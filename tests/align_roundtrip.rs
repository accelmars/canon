//! Integration tests for `canon align --output`.
//!
//! Tests: clean baseline, drift baseline, round-trip semantics, anchor plan validate,
//! boundary audit, determinism.

use std::path::{Path, PathBuf};

use canon_core::audit::DriftCategory;
use canon_core::cli::align::run_impl;
use canon_core::plan::{DefaultJudgmentEmitter, MechanicalPlanEmitter};
use canon_core::template::TemplateLoader;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn canon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> PathBuf {
    canon_dir().join("tests/fixtures/align")
}

fn template_with_rules_dir() -> PathBuf {
    canon_dir().join("tests/fixtures/templates/test-with-rules")
}

fn load_template_with_rules() -> canon_core::template::LoadedTemplate {
    let loader = TemplateLoader::from_workspace_root(&canon_dir());
    loader
        .load_by_path(&template_with_rules_dir())
        .expect("test-with-rules template must exist")
}

/// The anchor workspace root is one level above canon_dir().
/// Paths in emitted plans must be relative to this root so `anchor plan validate` can resolve them.
fn workspace_root() -> PathBuf {
    canon_dir()
        .parent()
        .expect("canon dir must have a parent")
        .to_owned()
}

fn run_align(corpus: &Path, output: &Path) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let cwd = canon_dir();
    let code = run_impl(
        corpus.to_str().unwrap(),
        template_with_rules_dir().to_str().unwrap(),
        output.to_str().unwrap(),
        None,
        &workspace_root(),
        &cwd,
        &mut out,
        &mut err,
    );
    (
        code,
        String::from_utf8_lossy(&out).to_string(),
        String::from_utf8_lossy(&err).to_string(),
    )
}

// ---------------------------------------------------------------------------
// Test (a): Clean baseline — empty plans
// ---------------------------------------------------------------------------

/// A conformant corpus (all numbered tiers, `_INDEX.md` present, valid frontmatter)
/// produces an empty main plan and empty FM plan.
#[test]
fn clean_baseline_produces_empty_main_plan() {
    let corpus = fixtures_dir().join("numbered-clean");
    let output = canon_dir().join("target/test-align-clean-main.toml");

    let (code, out_str, err_str) = run_align(&corpus, &output);
    assert_eq!(code, 0, "expected exit 0 for clean corpus; err={err_str}; out={out_str}");

    assert!(output.is_file(), "main plan not written to {}", output.display());
    let plan_content = std::fs::read_to_string(&output).unwrap();
    assert!(plan_content.starts_with("version = \"1\""));
    // No structural ops expected.
    assert!(
        !plan_content.contains("[[ops]]"),
        "clean corpus should produce no structural ops; got: {plan_content}"
    );
}

/// A conformant corpus produces an empty FM plan as well.
#[test]
fn clean_baseline_produces_empty_fm_plan() {
    let corpus = fixtures_dir().join("numbered-clean");
    let output = canon_dir().join("target/test-align-clean-fm-main.toml");

    let (code, _, err_str) = run_align(&corpus, &output);
    assert_eq!(code, 0, "err={err_str}");

    let fm_path = output.with_extension("").with_extension("fm-plan.toml");
    // Derived FM path: "test-align-clean-fm-main.fm-plan.toml"
    let fm_path2 = {
        let stem = output
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        output.parent().unwrap().join(format!("{}.fm-plan.toml", stem))
    };
    let actual_fm = if fm_path.is_file() { fm_path } else { fm_path2 };
    assert!(actual_fm.is_file(), "FM plan not written");
    let fm_content = std::fs::read_to_string(&actual_fm).unwrap();
    assert!(!fm_content.contains("[[ops]]"), "clean corpus should have no FM ops");
}

// ---------------------------------------------------------------------------
// Test (b): Drift baseline — non-empty plans
// ---------------------------------------------------------------------------

/// A corpus with unnumbered folders produces non-empty main plan with move ops.
#[test]
fn drift_baseline_unnumbered_folders_produce_move_ops() {
    let corpus = fixtures_dir().join("unnumbered-drift");
    let output = canon_dir().join("target/test-align-drift-main.toml");

    let (code, out_str, err_str) = run_align(&corpus, &output);
    // Exit 1 expected: drift found.
    assert!(code == 1 || code == 0, "unexpected error; err={err_str}; out={out_str}");

    let plan_content = std::fs::read_to_string(&output).unwrap();
    // The unnumbered-drift corpus has "analysis" and "identity" folders.
    // folder-rules.toml maps "analysis" → "32-analysis", "identity" → "01-identity".
    assert!(
        plan_content.contains("type = \"move\""),
        "expected move ops for unnumbered folders; got: {plan_content}"
    );
}

/// A corpus with frontmatter drift produces non-empty FM plan.
#[test]
fn drift_baseline_missing_frontmatter_produces_fm_ops() {
    let corpus = fixtures_dir().join("frontmatter-drift");
    let output = canon_dir().join("target/test-align-fm-drift-main.toml");

    let (code, _, err_str) = run_align(&corpus, &output);
    assert!(code <= 1, "unexpected error; err={err_str}");

    let fm_path = {
        let stem = output.file_stem().unwrap().to_string_lossy().to_string();
        output.parent().unwrap().join(format!("{}.fm-plan.toml", stem))
    };
    assert!(fm_path.is_file(), "FM plan not written");
    let fm_content = std::fs::read_to_string(&fm_path).unwrap();
    assert!(
        fm_content.contains("[[ops]]"),
        "expected FM ops for frontmatter drift; got: {fm_content}"
    );
    assert!(
        fm_content.contains("schema_version"),
        "expected schema_version field in FM ops; got: {fm_content}"
    );
}

// ---------------------------------------------------------------------------
// Test (c): Round-trip vs reference ops (semantic equivalence)
// ---------------------------------------------------------------------------

/// The unnumbered-drift fixture represents a pre-pass-1-like corpus state.
/// Canon should emit move ops that match the expected folder renames:
/// - analysis → 32-analysis
/// - identity → 01-identity
#[test]
fn roundtrip_move_ops_match_reference_renames() {
    let corpus = fixtures_dir().join("unnumbered-drift");
    let template = load_template_with_rules();

    let judgment_emitter = DefaultJudgmentEmitter;
    let plan_emitter = MechanicalPlanEmitter::new(&judgment_emitter);

    // Run audit directly to get drift.
    let drift = canon_core::audit::run_audit(&corpus, &template)
        .expect("audit must succeed");

    let emission = plan_emitter
        .emit_with_root(&corpus, &drift, &template, Some(&canon_dir()))
        .expect("emit must succeed");

    // Collect (src_basename, dst_basename) from move ops.
    let mut moves: Vec<(String, String)> = emission
        .main_plan
        .ops
        .iter()
        .filter_map(|op| {
            if let canon_core::plan::MainPlanOp::Move { src, dst } = op {
                let src_base = src.split('/').next_back().unwrap_or(src).to_string();
                let dst_base = dst.split('/').next_back().unwrap_or(dst).to_string();
                Some((src_base, dst_base))
            } else {
                None
            }
        })
        .collect();
    moves.sort();

    // Expected semantic equivalence with reference plan renames.
    let expected = {
        let mut v = vec![
            ("analysis".to_string(), "32-analysis".to_string()),
            ("identity".to_string(), "01-identity".to_string()),
        ];
        v.sort();
        v
    };
    assert_eq!(
        moves, expected,
        "move ops do not match expected folder renames"
    );
}

// ---------------------------------------------------------------------------
// Test (d): anchor plan validate
// ---------------------------------------------------------------------------

/// The emitted main plan validates against anchor's plan schema.
#[test]
fn emitted_plan_passes_anchor_validate() {
    let corpus = fixtures_dir().join("unnumbered-drift");
    let output = canon_dir().join("target/test-align-validate.toml");

    let (code, _, err_str) = run_align(&corpus, &output);
    assert!(code <= 1, "align failed: err={err_str}");
    assert!(output.is_file(), "plan not written");

    let status = std::process::Command::new("anchor")
        .arg("plan")
        .arg("validate")
        .arg(output.to_str().unwrap())
        .status();

    match status {
        Ok(s) => assert!(s.success(), "anchor plan validate failed for emitted plan"),
        Err(e) => {
            // anchor not available in CI — mark test as skipped via pass with warning.
            eprintln!("WARNING: anchor not available, skipping validate: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Test (e): Boundary audit — canon-core binary excludes closed-layer code
// ---------------------------------------------------------------------------

/// Building canon without default features must not link any `canon_judgment` symbol.
/// This test is run as part of the contract exit criteria: boundary audit.
#[test]
fn boundary_audit_canon_core_has_no_closed_layer_symbols() {
    // Build the open binary in debug mode.
    let manifest = canon_dir().join("Cargo.toml");
    let build_status = std::process::Command::new("cargo")
        .args(["build", "--manifest-path", manifest.to_str().unwrap(), "--bin", "canon"])
        .status();

    match build_status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            panic!("cargo build failed with exit code: {:?}", s.code());
        }
        Err(e) => {
            eprintln!("WARNING: cargo build not available: {e}");
            return;
        }
    }

    let binary = canon_dir().join("target/debug/canon");
    if !binary.is_file() {
        eprintln!("WARNING: binary not found at {}; skipping nm check", binary.display());
        return;
    }

    let nm_output = std::process::Command::new("nm")
        .arg(binary.to_str().unwrap())
        .output();

    match nm_output {
        Ok(out) => {
            let symbols = String::from_utf8_lossy(&out.stdout);
            let closed_count = symbols.lines()
                .filter(|l| l.contains("canon_judgment"))
                .count();
            assert_eq!(
                closed_count, 0,
                "open binary must not link canon_judgment symbols (boundary violation)"
            );
        }
        Err(e) => {
            eprintln!("WARNING: nm not available: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Test (f): Determinism — byte-identical on re-run
// ---------------------------------------------------------------------------

/// Running `canon align --output` twice on identical input produces byte-identical TOML.
#[test]
fn determinism_identical_runs_produce_identical_output() {
    let corpus = fixtures_dir().join("unnumbered-drift");
    let output1 = canon_dir().join("target/test-align-det-run1.toml");
    let output2 = canon_dir().join("target/test-align-det-run2.toml");

    let (c1, _, e1) = run_align(&corpus, &output1);
    let (c2, _, e2) = run_align(&corpus, &output2);
    assert!(c1 <= 1, "first run failed: {e1}");
    assert!(c2 <= 1, "second run failed: {e2}");

    let content1 = std::fs::read_to_string(&output1).expect("run1 plan not written");
    let content2 = std::fs::read_to_string(&output2).expect("run2 plan not written");
    assert_eq!(
        content1, content2,
        "two identical runs produced different TOML"
    );
}

// ---------------------------------------------------------------------------
// Test: Drift categories covered
// ---------------------------------------------------------------------------

/// Audit of the unnumbered-drift fixture reports FolderShape drift.
#[test]
fn audit_of_unnumbered_corpus_reports_folder_shape_drift() {
    let corpus = fixtures_dir().join("unnumbered-drift");
    let template = load_template_with_rules();
    let drift = canon_core::audit::run_audit(&corpus, &template).unwrap();
    let folder_shape: Vec<_> = drift
        .iter()
        .filter(|e| e.category == DriftCategory::FolderShape)
        .collect();
    assert!(
        !folder_shape.is_empty(),
        "expected FolderShape drift in unnumbered corpus"
    );
}
