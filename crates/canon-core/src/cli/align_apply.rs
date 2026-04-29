use std::io::Write;
use std::path::{Path, PathBuf};

use crate::orchestrator::anchor_runner::{AnchorMissingError, AnchorRunner, DefaultAnchorRunner};
use crate::orchestrator::{run_pipeline, OrchestratorConfig, OrchestratorOutcome};
use crate::template::{TemplateError, TemplateLoader};

// ---------------------------------------------------------------------------
// Anchor capability check (Step 7)
// ---------------------------------------------------------------------------

/// Verify `anchor frontmatter --help` exits 0 before running `--apply` mode.
///
/// Error message names AENG-006 so operators know which anchor version is required.
pub fn check_anchor_frontmatter() -> Result<(), AnchorMissingError> {
    crate::orchestrator::anchor_runner::check_anchor_frontmatter()
}

// ---------------------------------------------------------------------------
// Production entry point
// ---------------------------------------------------------------------------

/// Entry point for `canon align --apply [--gap-report-dir <path>]`.
///
/// Resolves workspace root from the environment. Returns exit code.
pub fn run(
    corpus_path_str: &str,
    template_spec: &str,
    apply: bool,
    gap_report_dir: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let workspace_root = find_workspace_root();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let anchor_runner = DefaultAnchorRunner;
    run_impl(
        corpus_path_str,
        template_spec,
        apply,
        gap_report_dir,
        &workspace_root,
        &cwd,
        &anchor_runner,
        out,
        err,
    )
}

/// Testable implementation — accepts explicit workspace_root, cwd, and anchor runner.
#[allow(clippy::too_many_arguments)]
pub fn run_impl(
    corpus_path_str: &str,
    template_spec: &str,
    apply: bool,
    gap_report_dir: Option<&str>,
    workspace_root: &Path,
    cwd: &Path,
    anchor_runner: &dyn AnchorRunner,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    // Resolve corpus path.
    let corpus_path = match resolve_src_path(corpus_path_str, workspace_root, cwd) {
        Some(p) => p,
        None => {
            let _ = writeln!(
                err,
                "error: corpus path '{}' not found\n\
                 Hint: paths are resolved from workspace root ({})",
                corpus_path_str,
                workspace_root.display()
            );
            return 2;
        }
    };

    // Load template.
    let loader = TemplateLoader::from_workspace_root(workspace_root);
    let template = if is_path_spec(template_spec) {
        loader.load_by_path(Path::new(template_spec))
    } else {
        loader.load_by_name(template_spec)
    };
    let template = match template {
        Ok(t) => t,
        Err(TemplateError::NotFound { name, searched }) => {
            let paths: Vec<_> = searched.iter().map(|p| p.display().to_string()).collect();
            let _ = writeln!(
                err,
                "error: template '{}' not found; searched:\n  {}",
                name,
                paths.join("\n  ")
            );
            return 2;
        }
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            return 2;
        }
    };

    // Anchor capability check — must pass before --apply mode runs.
    if apply {
        if let Err(e) = anchor_runner.check_frontmatter_capability() {
            let _ = writeln!(err, "error: {}", e.message);
            return 2;
        }
    }

    // Resolve gap-report directory.
    let resolved_gap_dir = resolve_gap_report_dir(gap_report_dir, &corpus_path, &template);

    let config = OrchestratorConfig {
        corpus_path,
        template,
        apply,
        gap_report_dir: resolved_gap_dir,
        workspace_root: workspace_root.to_owned(),
    };

    // Run pipeline.
    let outcome = run_pipeline(&config, anchor_runner);

    // Print summary.
    let mode = if apply { "apply" } else { "dry-run" };
    let _ = writeln!(out, "canon align --{mode}:");
    crate::orchestrator::print_pipeline_summary(&outcome, out);

    if let OrchestratorOutcome::Failed { error, .. } = &outcome {
        let _ = writeln!(err, "error: {}", error);
    }

    outcome.exit_code()
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

pub fn print_help(out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "USAGE:\n  canon align <corpus-path> --template <name|path> [--apply] \
         [--gap-report-dir <path>]\n\n\
         ARGS:\n  <corpus-path>   Directory to align\n\n\
         OPTIONS:\n\
           --template, -t        Template name or explicit path (required)\n\
           --apply               Execute via anchor + write gap report (default: dry-run)\n\
           --gap-report-dir      Gap report output directory \
                                 (default: template gaps_folder or '41-gaps')\n\n\
         EXIT CODES:\n  0  Conformant / apply succeeded with no residual drift\n\
           1  Drift found (plan available) or residual drift after apply\n  2  Error\n\n\
         NOTES:\n\
           Without --apply, prints a plan summary and gap-report preview.\n\
           With --apply, requires anchor v0.6.0+ (AENG-006). Invokes:\n\
             1. anchor apply <main-plan.toml>\n\
             2. anchor frontmatter migrate <fm-plan.toml>\n\
           Then writes gap files for judgment cases and re-audits for residual drift."
    );
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn resolve_src_path(path_str: &str, workspace_root: &Path, cwd: &Path) -> Option<PathBuf> {
    let p = Path::new(path_str);
    if p.is_absolute() {
        return if p.exists() { Some(p.to_owned()) } else { None };
    }
    let ws_rel = workspace_root.join(p);
    if ws_rel.exists() {
        return Some(ws_rel);
    }
    let cwd_rel = cwd.join(p);
    if cwd_rel.exists() {
        return Some(cwd_rel);
    }
    None
}

fn is_path_spec(spec: &str) -> bool {
    spec.starts_with('/') || spec.starts_with("./") || spec.starts_with("../")
}

/// Resolve gap-report directory: explicit flag → template invariant → "41-gaps" fallback.
fn resolve_gap_report_dir(
    explicit: Option<&str>,
    corpus_path: &Path,
    template: &crate::template::LoadedTemplate,
) -> PathBuf {
    if let Some(s) = explicit {
        return PathBuf::from(s);
    }
    let folder_name = template
        .manifest
        .invariants
        .as_ref()
        .and_then(|i| i.gaps_folder.as_deref())
        .unwrap_or("41-gaps");
    corpus_path.join(folder_name)
}

fn find_workspace_root() -> PathBuf {
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir: &Path = &cwd;
        loop {
            if dir.join(".accelmars").is_dir() {
                return dir.to_owned();
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::anchor_runner::MockAnchorRunner;
    use tempfile::TempDir;

    fn setup_minimal_workspace() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let tmpl_dir = dir.path().join(".accelmars/canon/templates/test-apply-cli");
        std::fs::create_dir_all(&tmpl_dir).unwrap();
        std::fs::write(
            tmpl_dir.join("manifest.toml"),
            r#"name = "test-apply-cli"
version = "1.0.0"
description = "test"
[folder_rules]
shape = "numbered-tiers"
"#,
        )
        .unwrap();
        let corpus = dir.path().join("corpus");
        std::fs::create_dir_all(corpus.join("01-identity")).unwrap();
        std::fs::write(corpus.join("01-identity/_INDEX.md"), "").unwrap();
        (dir, corpus)
    }

    #[test]
    fn dry_run_clean_corpus_exits_0() {
        let (dir, corpus) = setup_minimal_workspace();
        let runner = MockAnchorRunner::succeeds();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_impl(
            corpus.to_str().unwrap(),
            "test-apply-cli",
            false,
            None,
            dir.path(),
            dir.path(),
            &runner,
            &mut out,
            &mut err,
        );
        let err_str = String::from_utf8_lossy(&err);
        assert_eq!(code, 0, "err={err_str}");
        let out_str = String::from_utf8_lossy(&out);
        assert!(out_str.contains("dry-run"), "out={out_str}");
    }

    #[test]
    fn apply_anchor_missing_exits_2_with_message() {
        let (dir, corpus) = setup_minimal_workspace();
        let runner = MockAnchorRunner::anchor_missing();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_impl(
            corpus.to_str().unwrap(),
            "test-apply-cli",
            true,
            None,
            dir.path(),
            dir.path(),
            &runner,
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2);
        let err_str = String::from_utf8_lossy(&err);
        assert!(
            err_str.contains("AENG-006"),
            "expected AENG-006 in error, got: {err_str}"
        );
    }

    #[test]
    fn missing_corpus_exits_2() {
        let dir = TempDir::new().unwrap();
        let runner = MockAnchorRunner::succeeds();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_impl(
            "nonexistent-corpus",
            "test-apply-cli",
            false,
            None,
            dir.path(),
            dir.path(),
            &runner,
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2);
    }
}
