use std::path::PathBuf;

use canon_core::template::{TemplateTier, TemplateError, TemplateLoader};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates")
}

const TEST_BUILTIN_MANIFEST_TOML: &str =
    include_str!("fixtures/templates/test-builtin/manifest.toml");

fn test_loader() -> TemplateLoader {
    let fixtures = fixtures_dir();
    TemplateLoader::with_builtins(
        vec![("test-builtin".to_string(), TEST_BUILTIN_MANIFEST_TOML.to_string())],
        fixtures.clone(),
        fixtures,
    )
}

#[test]
fn builtin_load() {
    // Workspace and user dirs are nonexistent so lookup falls through to built-in.
    let loader = TemplateLoader::with_builtins(
        vec![("test-builtin".to_string(), TEST_BUILTIN_MANIFEST_TOML.to_string())],
        PathBuf::from("/nonexistent-workspace-dir"),
        PathBuf::from("/nonexistent-user-dir"),
    );
    let t = loader.load_by_name("test-builtin").expect("built-in template should load");
    assert_eq!(t.manifest.name, "test-builtin");
    assert_eq!(t.tier, TemplateTier::BuiltIn);
    assert!(t.dir.is_none(), "built-in should have no dir");
}

#[test]
fn workspace_load() {
    let loader = test_loader();
    let t = loader.load_by_name("test-workspace").expect("workspace template should load");
    assert_eq!(t.manifest.name, "test-workspace");
    assert_eq!(t.tier, TemplateTier::Workspace);
    assert!(t.dir.is_some(), "workspace template should have a dir");
}

#[test]
fn user_load() {
    let fixtures = fixtures_dir();
    let loader = TemplateLoader::with_builtins(
        vec![],
        PathBuf::from("/nonexistent-workspace-dir"),
        fixtures,
    );
    let t = loader.load_by_name("test-user").expect("user template should load");
    assert_eq!(t.manifest.name, "test-user");
    assert_eq!(t.tier, TemplateTier::User);
    assert!(t.dir.is_some());
}

#[test]
fn explicit_path_override() {
    let loader = test_loader();
    let path = fixtures_dir().join("test-workspace");
    let t = loader.load_by_path(&path).expect("explicit-path load should succeed");
    assert_eq!(t.manifest.name, "test-workspace");
    assert!(
        matches!(t.tier, TemplateTier::ExplicitPath(_)),
        "tier should be ExplicitPath"
    );
}

#[test]
fn missing_template_error_names_search_paths() {
    let loader = test_loader();
    let err = loader.load_by_name("does-not-exist").unwrap_err();
    match err {
        TemplateError::NotFound { name, searched } => {
            assert_eq!(name, "does-not-exist");
            assert!(!searched.is_empty(), "error should name at least one searched path");
        }
        other => panic!("expected NotFound, got: {other}"),
    }
}

#[test]
fn malformed_manifest_rejected_with_line_info() {
    let loader = TemplateLoader::with_builtins(
        vec![("bad".to_string(), "not valid toml %%$#@!\n[broken".to_string())],
        PathBuf::from("/nonexistent"),
        PathBuf::from("/nonexistent"),
    );
    let err = loader.load_by_name("bad").unwrap_err();
    let msg = err.to_string();
    assert!(
        matches!(err, TemplateError::Malformed { .. }),
        "should be Malformed, got: {msg}"
    );
    assert!(
        msg.contains("malformed template"),
        "error message should describe malformed template"
    );
}

#[test]
fn tier_precedence_workspace_wins_over_builtin() {
    let fixtures = fixtures_dir();
    // Register a built-in with the same name as the workspace fixture.
    let loader = TemplateLoader::with_builtins(
        vec![(
            "test-workspace".to_string(),
            TEST_BUILTIN_MANIFEST_TOML.to_string(),
        )],
        fixtures.clone(),
        fixtures,
    );
    let t = loader.load_by_name("test-workspace").expect("should load");
    assert_eq!(
        t.tier,
        TemplateTier::Workspace,
        "workspace should win over a built-in with the same name"
    );
    assert_eq!(t.manifest.name, "test-workspace");
}
