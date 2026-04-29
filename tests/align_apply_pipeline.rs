/// Integration tests for `canon align --apply` orchestrator pipeline (CFC-301).
///
/// Uses MockAnchorRunner throughout — anchor v0.6.0 (AENG-006) is a runtime
/// dependency for production `--apply`, not a test-time dependency.
use canon_core::orchestrator::anchor_runner::MockAnchorRunner;
use canon_core::orchestrator::{run_pipeline, OrchestratorConfig, StepOutcome};
use canon_core::template::TemplateLoader;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

/// Create a workspace with a named template.
///
/// Template has:
/// - numbered-tiers folder shape
/// - folder-rules.toml mapping "analysis" → "32-analysis"
/// - frontmatter schema requiring "title" field
fn setup_workspace_with_drift_template(dir: &TempDir) -> std::path::PathBuf {
    let tmpl_dir = dir.path().join(".accelmars/canon/templates/test-301");
    std::fs::create_dir_all(&tmpl_dir).unwrap();
    std::fs::write(
        tmpl_dir.join("manifest.toml"),
        r#"name = "test-301"
version = "1.0.0"
description = "CFC-301 test template"
[folder_rules]
shape = "numbered-tiers"
[frontmatter]
schema = "schema.json"
"#,
    )
    .unwrap();
    std::fs::write(
        tmpl_dir.join("folder-rules.toml"),
        r#"[[categories]]
number = "32"
folder = "32-analysis"
what = "Analysis"
layer = "universal"
"#,
    )
    .unwrap();
    std::fs::write(
        tmpl_dir.join("schema.json"),
        r#"{
  "type": "object",
  "required": ["title"],
  "properties": {
    "title": {"type": "string"}
  }
}"#,
    )
    .unwrap();
    tmpl_dir
}

/// Build a corpus with:
/// - `analysis/` folder (FolderShape drift → Move op)
/// - `analysis/note.md` missing required `title` field (FM drift)
/// - `big-file.md` > 500 lines (GraduationCandidate gap row)
fn setup_drift_corpus(dir: &TempDir) -> std::path::PathBuf {
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("analysis")).unwrap();
    std::fs::write(
        corpus.join("analysis/note.md"),
        "---\ntype: note\n---\n# Note\nSome content.\n",
    )
    .unwrap();
    // Large file triggers GraduationCandidate heuristic (> 500 lines).
    let big_content = "# Big File\n".to_string() + &"line of content\n".repeat(510);
    std::fs::write(corpus.join("big-file.md"), &big_content).unwrap();
    corpus
}

/// Build a clean corpus: conformant numbered folders with _INDEX.md files.
///
/// `41-gaps/` is included as a pre-initialized numbered folder so the audit
/// does not report `MissingIndex` drift on re-audit after the orchestrator runs.
fn setup_clean_corpus(dir: &TempDir) -> std::path::PathBuf {
    let corpus = dir.path().join("corpus");
    std::fs::create_dir_all(corpus.join("01-identity")).unwrap();
    std::fs::write(
        corpus.join("01-identity/_INDEX.md"),
        "---\ntitle: Identity\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(corpus.join("41-gaps")).unwrap();
    std::fs::write(
        corpus.join("41-gaps/_INDEX.md"),
        "---\ntitle: Gaps\n---\n",
    )
    .unwrap();
    corpus
}

fn load_test_template(dir: &TempDir) -> canon_core::template::LoadedTemplate {
    let loader = TemplateLoader::from_workspace_root(dir.path());
    loader.load_by_name("test-301").unwrap()
}

// ---------------------------------------------------------------------------
// Test 8a: Dry-run on drift fixture — plan summary + gap preview; nothing on disk
// ---------------------------------------------------------------------------

#[test]
fn dry_run_drift_fixture_nothing_written_to_disk() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let corpus = setup_drift_corpus(&dir);
    let template = load_test_template(&dir);
    let gap_dir = corpus.join("41-gaps");

    let config = OrchestratorConfig {
        corpus_path: corpus.clone(),
        template,
        apply: false,
        gap_report_dir: gap_dir.clone(),
        workspace_root: dir.path().to_owned(),
    };
    let runner = MockAnchorRunner::succeeds();
    let outcome = run_pipeline(&config, &runner);

    // ApplySkipped step present — confirms dry-run path.
    let skipped = outcome.steps().iter().any(|s| matches!(s, StepOutcome::ApplySkipped));
    assert!(skipped, "dry-run must have ApplySkipped step");

    // Nothing written to disk.
    assert!(
        !gap_dir.exists(),
        "gap dir must not be created in dry-run; path={}", gap_dir.display()
    );

    // Plan step shows non-zero drift.
    let plan_step = outcome.steps().iter().find(|s| matches!(s, StepOutcome::Plan { .. }));
    assert!(plan_step.is_some(), "Plan step must be present");

    // Exit code 1: corpus has blocking drift (FolderShape, FM missing).
    assert_eq!(outcome.exit_code(), 1, "dry-run with drift should exit 1");
}

// ---------------------------------------------------------------------------
// Test 8b: Apply on small fixture — structural rename + FM migration + 1 graduation
// ---------------------------------------------------------------------------

#[test]
fn apply_small_fixture_produces_gap_file() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let corpus = setup_drift_corpus(&dir);
    let template = load_test_template(&dir);
    let gap_dir = corpus.join("41-gaps");
    std::fs::create_dir_all(&gap_dir).unwrap();

    let config = OrchestratorConfig {
        corpus_path: corpus.clone(),
        template,
        apply: true,
        gap_report_dir: gap_dir.clone(),
        workspace_root: dir.path().to_owned(),
    };
    let runner = MockAnchorRunner::succeeds();
    let outcome = run_pipeline(&config, &runner);

    // Pipeline should have AnchorApply, AnchorFmMigrate, GapReport, ReAudit steps.
    let has_apply = outcome.steps().iter().any(|s| matches!(s, StepOutcome::AnchorApply { .. }));
    let has_fm = outcome.steps().iter().any(|s| matches!(s, StepOutcome::AnchorFmMigrate { .. }));
    let has_gap = outcome.steps().iter().any(|s| matches!(s, StepOutcome::GapReport { .. }));
    let has_reaudit = outcome.steps().iter().any(|s| matches!(s, StepOutcome::ReAudit { .. }));
    assert!(has_apply, "missing AnchorApply step");
    assert!(has_fm, "missing AnchorFmMigrate step");
    assert!(has_gap, "missing GapReport step; steps={:?}", outcome.steps());
    assert!(has_reaudit, "missing ReAudit step");

    // At least one gap file produced for the graduation candidate.
    let canon_files: Vec<_> = std::fs::read_dir(&gap_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
        .collect();
    assert!(
        !canon_files.is_empty(),
        "expected ≥1 gap file in {}", gap_dir.display()
    );

    // Gap file contains required frontmatter.
    let content = std::fs::read_to_string(canon_files[0].path()).unwrap();
    assert!(content.contains("type: gap"), "gap file missing 'type: gap'");
    assert!(content.contains("engine: canon"), "gap file missing 'engine: canon'");
    assert!(
        content.contains("source: canon-template-architecture-CFC-301"),
        "gap file missing source"
    );
}

// ---------------------------------------------------------------------------
// Test 8c: Idempotence — clean corpus + --apply is a no-op
// ---------------------------------------------------------------------------

#[test]
fn idempotence_clean_corpus_apply_is_noop() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let corpus = setup_clean_corpus(&dir); // creates 41-gaps/_INDEX.md
    let template = load_test_template(&dir);
    let gap_dir = corpus.join("41-gaps"); // already exists from setup_clean_corpus

    let config = OrchestratorConfig {
        corpus_path: corpus.clone(),
        template,
        apply: true,
        gap_report_dir: gap_dir.clone(),
        workspace_root: dir.path().to_owned(),
    };
    let runner = MockAnchorRunner::succeeds();
    let outcome = run_pipeline(&config, &runner);

    // Exit 0: no drift.
    assert_eq!(outcome.exit_code(), 0, "clean corpus + apply must exit 0");

    // Zero plan ops.
    let plan = outcome.steps().iter().find_map(|s| {
        if let StepOutcome::Plan { main_ops, fm_ops, gap_rows } = s {
            Some((*main_ops, *fm_ops, *gap_rows))
        } else {
            None
        }
    });
    assert_eq!(plan, Some((0, 0, 0)), "clean corpus must produce zero plan ops");

    // Zero gap files written.
    let canon_files: Vec<_> = std::fs::read_dir(&gap_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
        .collect();
    assert!(
        canon_files.is_empty(),
        "clean corpus must produce zero gap files"
    );

    // ReAudit shows zero residual drift.
    let reaudit = outcome.steps().iter().find_map(|s| {
        if let StepOutcome::ReAudit { residual_drift } = s {
            Some(*residual_drift)
        } else {
            None
        }
    });
    assert_eq!(reaudit, Some(0), "re-audit must show zero residual drift");
}

// ---------------------------------------------------------------------------
// Test 8d: Anchor missing — capability check fails, exit 2 with clear message
// ---------------------------------------------------------------------------

#[test]
fn anchor_missing_errors_with_aeng006_message() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let corpus = setup_clean_corpus(&dir);
    let _template = load_test_template(&dir);

    // Check via the CLI subcommand (which performs the capability gate).
    use canon_core::cli::align_apply::run_impl;
    let runner = MockAnchorRunner::anchor_missing();
    let mut out = Vec::new();
    let mut err_buf = Vec::new();
    let code = run_impl(
        corpus.to_str().unwrap(),
        "test-301",
        true, // --apply
        None,
        dir.path(),
        dir.path(),
        &runner,
        &mut out,
        &mut err_buf,
    );
    assert_eq!(code, 2, "anchor-missing must exit 2");
    let err_str = String::from_utf8_lossy(&err_buf);
    assert!(
        err_str.contains("AENG-006"),
        "error must name AENG-006; got: {err_str}"
    );
}

// ---------------------------------------------------------------------------
// Test 8e: Apply failure — MockAnchorRunner returns exit 1 + AENG-002 diagnostic
// ---------------------------------------------------------------------------

#[test]
fn apply_failure_propagates_diagnostic_and_exit_nonzero() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let corpus = setup_drift_corpus(&dir);
    let template = load_test_template(&dir);
    let gap_dir = corpus.join("41-gaps");

    let config = OrchestratorConfig {
        corpus_path: corpus,
        template,
        apply: true,
        gap_report_dir: gap_dir,
        workspace_root: dir.path().to_owned(),
    };

    let diagnostic = "AENG-002: broken reference in analysis/note.md → 01-identity/_INDEX.md";
    let runner = MockAnchorRunner::apply_fails(diagnostic);
    let outcome = run_pipeline(&config, &runner);

    // Must exit non-zero.
    assert!(outcome.exit_code() != 0, "apply failure must exit non-zero");

    // AnchorApplyFailed step present with the diagnostic.
    let failed_step = outcome.steps().iter().find_map(|s| {
        if let StepOutcome::AnchorApplyFailed { diagnostic: d, .. } = s {
            Some(d.clone())
        } else {
            None
        }
    });
    assert!(failed_step.is_some(), "AnchorApplyFailed step not found");
    assert!(
        failed_step.unwrap().contains("AENG-002"),
        "diagnostic must contain AENG-002"
    );

    // Pipeline stopped — no GapReport step should follow a failed apply.
    let has_gap = outcome.steps().iter().any(|s| matches!(s, StepOutcome::GapReport { .. }));
    assert!(!has_gap, "GapReport must not be written after a failed apply");
}

// ---------------------------------------------------------------------------
// Test 8f: Gap-report naming — second run continues NNN from first run
// ---------------------------------------------------------------------------

#[test]
fn gap_report_naming_continues_across_runs() {
    let dir = TempDir::new().unwrap();
    setup_workspace_with_drift_template(&dir);
    let gap_dir = dir.path().join("41-gaps");
    std::fs::create_dir_all(&gap_dir).unwrap();

    let make_config = |dir: &TempDir, gap_dir: &std::path::Path| -> OrchestratorConfig {
        let corpus = dir.path().join("corpus-naming");
        std::fs::create_dir_all(corpus.join("analysis")).unwrap();
        std::fs::write(
            corpus.join("analysis/note.md"),
            "---\ntype: note\n---\n",
        )
        .unwrap();
        let big = "# Big\n".to_string() + &"x\n".repeat(510);
        std::fs::write(corpus.join("big.md"), &big).unwrap();
        let loader = TemplateLoader::from_workspace_root(dir.path());
        let template = loader.load_by_name("test-301").unwrap();
        OrchestratorConfig {
            corpus_path: corpus,
            template,
            apply: true,
            gap_report_dir: gap_dir.to_owned(),
            workspace_root: dir.path().to_owned(),
        }
    };

    // Run 1.
    let config1 = make_config(&dir, &gap_dir);
    let runner = MockAnchorRunner::succeeds();
    run_pipeline(&config1, &runner);

    let after_run1: Vec<_> = std::fs::read_dir(&gap_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
        .collect();
    let count_after_run1 = after_run1.len();
    assert!(count_after_run1 > 0, "run 1 must produce at least one gap file");

    // Run 2 (same gap dir — should continue numbering).
    let config2 = make_config(&dir, &gap_dir);
    let runner2 = MockAnchorRunner::succeeds();
    run_pipeline(&config2, &runner2);

    let after_run2: Vec<_> = std::fs::read_dir(&gap_dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
        .collect();
    assert_eq!(
        after_run2.len(),
        count_after_run1 * 2,
        "run 2 must add the same number of gap files, total doubling"
    );

    // Check that run-2 files have higher NNN than run-1 files.
    let mut names: Vec<_> = after_run2
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();

    // The latter half should have higher numbers.
    let max_run1_n = count_after_run1 as u32;
    let second_half_start_name = &names[count_after_run1];
    let second_n: u32 = second_half_start_name
        .strip_prefix("CANON-")
        .and_then(|s| s.split('-').next())
        .and_then(|n| n.parse().ok())
        .expect("second run file name must parse");
    assert!(
        second_n > max_run1_n,
        "second run NNN ({second_n}) must be higher than first run max ({max_run1_n})"
    );
}
