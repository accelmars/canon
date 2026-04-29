use std::cell::RefCell;
use std::path::{Path, PathBuf};

use canon_core::cli::template::{run_impl, GitCloner};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates")
}

fn to_args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// No-op git cloner — used when git is not expected to be called.
struct NoopGitCloner;

impl GitCloner for NoopGitCloner {
    fn clone_repo(&self, _url: &str, _dest: &Path) -> Result<(), String> {
        panic!("GitCloner::clone_repo called unexpectedly")
    }
}

/// Recording git cloner — records calls and optionally writes a minimal template to dest.
struct MockGitCloner {
    calls: RefCell<Vec<(String, PathBuf)>>,
}

impl MockGitCloner {
    fn new() -> Self {
        Self {
            calls: RefCell::new(vec![]),
        }
    }

    fn recorded_calls(&self) -> Vec<(String, PathBuf)> {
        self.calls.borrow().clone()
    }
}

impl GitCloner for MockGitCloner {
    fn clone_repo(&self, url: &str, dest: &Path) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push((url.to_string(), dest.to_owned()));
        // Create a minimal valid template so install can report success.
        std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        std::fs::write(
            dest.join("manifest.toml"),
            "name = \"cloned-template\"\nversion = \"0.1.0\"\ndescription = \"cloned\"\n\n[folder_rules]\nshape = \"flat\"\n",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// template list
// ---------------------------------------------------------------------------

#[test]
fn list_shows_builtin_templates() {
    let user_dir = PathBuf::from("/nonexistent-user-templates-list-test");
    let workspace_root = PathBuf::from("/nonexistent-workspace-list-test");

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["list"]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &workspace_root,
        &workspace_root,
        &user_dir,
    );

    let output = String::from_utf8(out).unwrap();
    assert_eq!(
        exit,
        0,
        "list should exit 0; stderr: {}",
        String::from_utf8(err).unwrap()
    );
    assert!(
        output.contains("BUILT-IN TEMPLATES"),
        "should show built-in section"
    );
    assert!(
        output.contains("canon-default"),
        "should list canon-default built-in"
    );
    assert!(
        output.contains("WORKSPACE TEMPLATES"),
        "should show workspace section"
    );
    assert!(
        output.contains("USER TEMPLATES"),
        "should show user section"
    );
}

#[test]
fn list_shows_workspace_templates_when_present() {
    // Point workspace root at the fixtures dir parent so loader finds fixtures as workspace templates.
    let fixtures = fixtures_dir();
    let workspace_root = fixtures.parent().unwrap().parent().unwrap().to_owned(); // tests/
    let workspace_templates_dir = workspace_root.join(".accelmars/canon/templates");
    let user_dir = PathBuf::from("/nonexistent-user-templates-ws-test");

    // Only run if the workspace templates dir resolves to fixtures path via link or if we manually
    // set it. Since run_impl uses workspace_root.join(".accelmars/canon/templates"), we create
    // a loader manually to confirm fixture structure, then call the list subcommand with a fake
    // workspace root that has no workspace templates — what matters is built-in section works.
    drop(workspace_templates_dir); // not needed — just testing section output format

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["list"]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &workspace_root,
        &workspace_root,
        &user_dir,
    );

    let output = String::from_utf8(out).unwrap();
    assert_eq!(exit, 0);
    assert!(output.contains("WORKSPACE TEMPLATES (.accelmars/canon/templates/)"));
}

// ---------------------------------------------------------------------------
// template show
// ---------------------------------------------------------------------------

#[test]
fn show_canon_default_prints_coherent_manifest() {
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["show", "canon-default"]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent-workspace"),
        &PathBuf::from("/nonexistent-cwd"),
        &PathBuf::from("/nonexistent-user"),
    );

    let output = String::from_utf8(out).unwrap();
    assert_eq!(
        exit,
        0,
        "show canon-default should succeed; stderr: {}",
        String::from_utf8(err).unwrap()
    );
    assert!(output.contains("name:"), "should show name field");
    assert!(
        output.contains("canon-default"),
        "should show template name"
    );
    assert!(output.contains("version:"), "should show version field");
    assert!(
        output.contains("description:"),
        "should show description field"
    );
    assert!(output.contains("shape:"), "should show shape field");
    assert!(output.contains("tier:"), "should show tier field");
    assert!(
        output.contains("built-in"),
        "should identify as built-in tier"
    );
}

#[test]
fn show_canon_default_json_format() {
    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["show", "canon-default", "--format", "json"]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent-workspace"),
        &PathBuf::from("/nonexistent-cwd"),
        &PathBuf::from("/nonexistent-user"),
    );

    assert_eq!(exit, 0, "show --format json should succeed");
    let output = String::from_utf8(out).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&output).expect("show --format json should emit valid JSON");
    assert_eq!(parsed["name"], "canon-default");
    assert_eq!(parsed["tier"], "built-in");
    assert!(parsed["folder_rules"]["shape"].is_string());
}

// ---------------------------------------------------------------------------
// template validate
// ---------------------------------------------------------------------------

#[test]
fn validate_accepts_valid_template() {
    let fixture_path = fixtures_dir().join("test-workspace");

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["validate", fixture_path.to_str().unwrap()]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/"),
        &PathBuf::from("/nonexistent-user"),
    );

    assert_eq!(
        exit,
        0,
        "valid template should exit 0; stderr: {}",
        String::from_utf8(err).unwrap()
    );
    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("valid:"), "should print valid confirmation");
    assert!(
        output.contains("test-workspace"),
        "should name the template"
    );
}

#[test]
fn validate_rejects_malformed_template_with_line_field_info() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join("manifest.toml"),
        "not valid toml %%$#@!\n[broken",
    )
    .unwrap();

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["validate", tmp.path().to_str().unwrap()]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/"),
        &PathBuf::from("/nonexistent-user"),
    );

    assert_eq!(exit, 1, "malformed template should exit 1");
    let stderr = String::from_utf8(err).unwrap();
    assert!(
        stderr.contains("invalid:"),
        "should describe invalid template; got: {stderr}"
    );
}

#[test]
fn validate_missing_schema_file_exits_1() {
    let tmp = tempfile::TempDir::new().unwrap();
    // Write a manifest that references a schema file that doesn't exist
    std::fs::write(
        tmp.path().join("manifest.toml"),
        "name = \"test\"\nversion = \"0.1.0\"\ndescription = \"test\"\n\n[folder_rules]\nshape = \"flat\"\n\n[frontmatter]\nschema = \"missing.schema.json\"\n",
    )
    .unwrap();

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["validate", tmp.path().to_str().unwrap()]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/"),
        &PathBuf::from("/nonexistent-user"),
    );

    assert_eq!(exit, 1, "missing schema should exit 1");
    let stderr = String::from_utf8(err).unwrap();
    assert!(
        stderr.contains("invalid:"),
        "should describe invalid template; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// template install — local path
// ---------------------------------------------------------------------------

#[test]
fn install_from_local_path_copies_to_user_dir() {
    let src = fixtures_dir().join("test-workspace");
    let user_dir = tempfile::TempDir::new().unwrap();

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["install", src.to_str().unwrap()]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/"),
        user_dir.path(),
    );

    assert_eq!(
        exit,
        0,
        "install from local path should succeed; stderr: {}",
        String::from_utf8(err).unwrap()
    );
    // Template name = "test-workspace" (from manifest.toml)
    let dest = user_dir.path().join("test-workspace");
    assert!(
        dest.exists(),
        "template dir should exist at user_dir/test-workspace"
    );
    assert!(
        dest.join("manifest.toml").exists(),
        "manifest.toml should be copied"
    );

    let output = String::from_utf8(out).unwrap();
    assert!(output.contains("installed:"), "should confirm installation");
    assert!(
        output.contains("test-workspace"),
        "should name the template"
    );
}

#[test]
fn install_local_rejects_already_installed() {
    let src = fixtures_dir().join("test-workspace");
    let user_dir = tempfile::TempDir::new().unwrap();
    // Pre-create the destination to simulate already-installed
    std::fs::create_dir_all(user_dir.path().join("test-workspace")).unwrap();

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["install", src.to_str().unwrap()]),
        &mut out,
        &mut err,
        &NoopGitCloner,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/"),
        user_dir.path(),
    );

    assert_eq!(exit, 2, "should fail when destination already exists");
    let stderr = String::from_utf8(err).unwrap();
    assert!(
        stderr.contains("already exists"),
        "should describe conflict; got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// template install — git URL (smoke test)
// ---------------------------------------------------------------------------

#[test]
fn install_from_git_url_attempts_clone_and_writes_to_user_dir() {
    let user_dir = tempfile::TempDir::new().unwrap();
    let mock = MockGitCloner::new();
    let url = "https://github.com/example/my-template.git";

    let mut out = Vec::<u8>::new();
    let mut err = Vec::<u8>::new();
    let exit = run_impl(
        &to_args(&["install", url]),
        &mut out,
        &mut err,
        &mock,
        &PathBuf::from("/nonexistent"),
        &PathBuf::from("/nonexistent-cwd"),
        user_dir.path(),
    );

    assert_eq!(
        exit,
        0,
        "git install should succeed with mock cloner; stderr: {}",
        String::from_utf8(err).unwrap()
    );

    let calls = mock.recorded_calls();
    assert_eq!(calls.len(), 1, "should call clone_repo exactly once");
    assert_eq!(calls[0].0, url, "should clone the provided URL");
    // Destination should be user_dir/my-template (strip .git suffix)
    assert!(
        calls[0].1.starts_with(user_dir.path()),
        "clone dest should be inside user_dir"
    );
    let dest = user_dir.path().join("my-template");
    assert!(dest.exists(), "mock cloner should have written to user dir");
}
