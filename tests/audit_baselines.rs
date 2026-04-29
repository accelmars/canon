use std::path::PathBuf;

use canon_core::audit::{run_audit, DriftCategory};
use canon_core::cli::audit::{run_impl, OutputFormat};
use canon_core::template::TemplateLoader;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The canon repo root (CARGO_MANIFEST_DIR).
fn canon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The workspace root — parent of canon repo dir.
fn workspace_root() -> PathBuf {
    canon_dir()
        .parent()
        .expect("canon dir must have a parent")
        .to_owned()
}

fn fixtures_dir() -> PathBuf {
    canon_dir().join("tests/fixtures/audit")
}

fn template_loader() -> TemplateLoader {
    TemplateLoader::from_workspace_root(&workspace_root())
}

/// Run `run_impl` against a named fixture directory, using real workspace_root for
/// template resolution.
fn audit_fixture(fixture_name: &str, template_spec: &str, format: &OutputFormat) -> i32 {
    let corpus = fixtures_dir().join(fixture_name);
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    run_impl(
        corpus.to_str().unwrap(),
        template_spec,
        format,
        &ws,
        &ws,
        &mut out,
        &mut err,
    )
}

// ---------------------------------------------------------------------------
// Clean baseline (synthetic fixture — exits 0, no blocking drift)
// ---------------------------------------------------------------------------

#[test]
fn clean_baseline_exits_0() {
    let code = audit_fixture("clean-corpus", "accelmars-standard", &OutputFormat::Table);
    assert_eq!(code, 0, "clean-corpus should produce no blocking drift");
}

#[test]
fn clean_baseline_run_audit_returns_no_blocking_entries() {
    let corpus = fixtures_dir().join("clean-corpus");
    let loader = template_loader();
    let template = loader
        .load_by_name("accelmars-standard")
        .expect("accelmars-standard must resolve");
    let entries = run_audit(&corpus, &template).expect("audit must not error");
    let blocking: Vec<_> = entries
        .iter()
        .filter(|e| !e.category.is_informational())
        .collect();
    assert!(
        blocking.is_empty(),
        "clean-corpus should have no blocking drift; got: {:?}",
        blocking.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Drift baseline (gateway-engine — exits 1, has blocking drift)
// ---------------------------------------------------------------------------

#[test]
fn drift_baseline_gateway_engine_exits_1() {
    let gateway = workspace_root().join("accelmars-workspace/foundations/gateway-engine");
    if !gateway.is_dir() {
        eprintln!("gateway-engine not found at {:?} — skipping", gateway);
        return;
    }
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let code = run_impl(
        gateway.to_str().unwrap(),
        "accelmars-standard",
        &OutputFormat::Table,
        &ws,
        &ws,
        &mut out,
        &mut err,
    );
    assert_eq!(code, 1, "gateway-engine should have blocking drift");
}

// ---------------------------------------------------------------------------
// Synthetic fixtures — each DriftCategory must fire in at least one test
// ---------------------------------------------------------------------------

#[test]
fn folder_shape_drift_fires() {
    let corpus = fixtures_dir().join("folder-shape-drift");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::FolderShape),
        "FolderShape must fire for folder-shape-drift fixture"
    );
    assert_eq!(
        audit_fixture(
            "folder-shape-drift",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

#[test]
fn missing_index_drift_fires() {
    let corpus = fixtures_dir().join("missing-index");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::MissingIndex),
        "MissingIndex must fire for missing-index fixture"
    );
    assert_eq!(
        audit_fixture("missing-index", "accelmars-standard", &OutputFormat::Table),
        1
    );
}

#[test]
fn frontmatter_required_missing_fires() {
    let corpus = fixtures_dir().join("frontmatter-required-missing");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::FrontmatterRequiredMissing),
        "FrontmatterRequiredMissing must fire"
    );
    assert_eq!(
        audit_fixture(
            "frontmatter-required-missing",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

#[test]
fn frontmatter_type_wrong_fires() {
    let corpus = fixtures_dir().join("frontmatter-type-wrong");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::FrontmatterTypeWrong),
        "FrontmatterTypeWrong must fire"
    );
    assert_eq!(
        audit_fixture(
            "frontmatter-type-wrong",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

#[test]
fn frontmatter_value_invalid_fires() {
    let corpus = fixtures_dir().join("frontmatter-value-invalid");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::FrontmatterValueInvalid),
        "FrontmatterValueInvalid must fire"
    );
    assert_eq!(
        audit_fixture(
            "frontmatter-value-invalid",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

#[test]
fn graduation_candidate_fires() {
    let corpus = fixtures_dir().join("graduation-candidate");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::GraduationCandidate),
        "GraduationCandidate must fire for file > 500 lines"
    );
    // GraduationCandidate is blocking — exit 1.
    assert_eq!(
        audit_fixture(
            "graduation-candidate",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

#[test]
fn content_split_suggested_fires() {
    let corpus = fixtures_dir().join("content-split");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::ContentSplitSuggested),
        "ContentSplitSuggested must fire for file with 4+ H2 sections"
    );
    assert_eq!(
        audit_fixture("content-split", "accelmars-standard", &OutputFormat::Table),
        1
    );
}

#[test]
fn unknown_field_info_fires() {
    let corpus = fixtures_dir().join("unknown-field");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::UnknownFieldInfo),
        "UnknownFieldInfo must fire for unknown frontmatter field"
    );
    // UnknownFieldInfo is informational — exits 0.
    assert_eq!(
        audit_fixture("unknown-field", "accelmars-standard", &OutputFormat::Table),
        0,
        "unknown field is informational and must not cause exit 1"
    );
}

#[test]
fn invariant_violation_fires() {
    let corpus = fixtures_dir().join("invariant-violation");
    let loader = template_loader();
    let template = loader.load_by_name("accelmars-standard").unwrap();
    let entries = run_audit(&corpus, &template).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e.category == DriftCategory::InvariantViolation),
        "InvariantViolation must fire when gaps_folder is a file"
    );
    assert_eq!(
        audit_fixture(
            "invariant-violation",
            "accelmars-standard",
            &OutputFormat::Table
        ),
        1
    );
}

// ---------------------------------------------------------------------------
// Error paths
// ---------------------------------------------------------------------------

#[test]
fn nonexistent_corpus_exits_2() {
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let code = run_impl(
        "/tmp/does-not-exist-canon-test-99999",
        "canon-default",
        &OutputFormat::Table,
        &ws,
        &ws,
        &mut out,
        &mut err,
    );
    assert_eq!(code, 2, "nonexistent corpus must exit 2");
    let err_str = String::from_utf8(err).unwrap();
    assert!(
        err_str.contains("not found"),
        "error message must mention 'not found'"
    );
}

#[test]
fn nonexistent_template_exits_2() {
    let corpus = fixtures_dir().join("clean-corpus");
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let code = run_impl(
        corpus.to_str().unwrap(),
        "does-not-exist-template-abc123",
        &OutputFormat::Table,
        &ws,
        &ws,
        &mut out,
        &mut err,
    );
    assert_eq!(code, 2, "nonexistent template must exit 2");
}

// ---------------------------------------------------------------------------
// Format flags
// ---------------------------------------------------------------------------

#[test]
fn format_json_produces_parseable_json() {
    let corpus = fixtures_dir().join("folder-shape-drift");
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let code = run_impl(
        corpus.to_str().unwrap(),
        "accelmars-standard",
        &OutputFormat::Json,
        &ws,
        &ws,
        &mut out,
        &mut err,
    );
    assert_ne!(code, 2, "JSON format must not produce an error");
    let output_str = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&output_str).expect("JSON output must be parseable");
    assert!(
        parsed.get("entries").is_some(),
        "JSON output must contain an 'entries' key"
    );
    assert!(
        parsed.get("blocking_count").is_some(),
        "JSON output must contain a 'blocking_count' key"
    );
}

#[test]
fn format_markdown_produces_table() {
    let corpus = fixtures_dir().join("folder-shape-drift");
    let ws = workspace_root();
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let code = run_impl(
        corpus.to_str().unwrap(),
        "accelmars-standard",
        &OutputFormat::Markdown,
        &ws,
        &ws,
        &mut out,
        &mut err,
    );
    assert_ne!(code, 2, "markdown format must not produce an error");
    let output_str = String::from_utf8(out).unwrap();
    assert!(
        output_str.contains("| File |"),
        "markdown output must contain a table header"
    );
}
