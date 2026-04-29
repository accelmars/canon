use std::io::Write;
use std::path::{Path, PathBuf};

use crate::audit;
use crate::plan::{DefaultJudgmentEmitter, MechanicalPlanEmitter};
use crate::template::{TemplateError, TemplateLoader};

/// Production entry point for `canon align --output`.
///
/// Resolves workspace root and CWD from the environment.
/// Returns exit code: 0 = success, 1 = drift found (plan written), 2 = error.
pub fn run(
    corpus_path_str: &str,
    template_spec: &str,
    output_path: &str,
    fm_output_path: Option<&str>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let workspace_root = find_workspace_root();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    run_impl(
        corpus_path_str,
        template_spec,
        output_path,
        fm_output_path,
        &workspace_root,
        &cwd,
        out,
        err,
    )
}

/// Testable implementation — accepts explicit workspace_root, cwd, and writers.
///
/// Follows Rule 13: no global state, no set_current_dir, no env-var side effects.
#[allow(clippy::too_many_arguments)]
pub fn run_impl(
    corpus_path_str: &str,
    template_spec: &str,
    output_path_str: &str,
    fm_output_path_str: Option<&str>,
    workspace_root: &Path,
    cwd: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    // Resolve corpus path per Rule 13: workspace-root-relative first, then CWD-relative.
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

    // Output paths: always resolve relative to CWD (Rule 13 — dst paths).
    let output_path = resolve_dst_path(output_path_str, cwd);
    let fm_output_path = fm_output_path_str.map(|s| resolve_fm_default(s, &output_path, cwd));

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

    // Run audit to get drift report.
    let drift = match audit::run_audit(&corpus_path, &template) {
        Ok(e) => e,
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            return 2;
        }
    };

    // Emit plans.
    let judgment_emitter = DefaultJudgmentEmitter;
    let plan_emitter = MechanicalPlanEmitter::new(&judgment_emitter);

    let emission =
        match plan_emitter.emit_with_root(&corpus_path, &drift, &template, Some(workspace_root)) {
            Ok(e) => e,
            Err(e) => {
                let _ = writeln!(err, "error: {}", e);
                return 2;
            }
        };

    // Write main plan.
    if let Err(e) = plan_emitter.write_main_plan(&emission, &output_path) {
        let _ = writeln!(
            err,
            "error writing plan to '{}': {}",
            output_path.display(),
            e
        );
        return 2;
    }

    // Write FM plan (to the derived or explicit path).
    let fm_path = fm_output_path.unwrap_or_else(|| derive_fm_path(&output_path));
    if let Err(e) = plan_emitter.write_fm_plan(&emission, &fm_path) {
        let _ = writeln!(
            err,
            "error writing FM plan to '{}': {}",
            fm_path.display(),
            e
        );
        return 2;
    }

    // Report summary to stdout.
    let blocking = audit::has_blocking_drift(&drift);
    let main_ops = emission.main_plan.ops.len();
    let fm_ops = emission.fm_plan.ops.len();
    let gap_rows = emission.gap_rows.len();

    let _ = writeln!(
        out,
        "canon align: wrote plan to '{}' ({} structural ops, {} FM ops, {} gap rows)",
        output_path.display(),
        main_ops,
        fm_ops,
        gap_rows,
    );
    if !emission.gap_rows.is_empty() {
        let _ = writeln!(out, "\ngap rows (require human judgment):");
        for row in &emission.gap_rows {
            let _ = writeln!(
                out,
                "  [{}] {} — {}",
                row.category.as_str(),
                row.path.display(),
                row.description
            );
        }
    }

    if blocking {
        1
    } else {
        0
    }
}

pub fn print_help(out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "USAGE:\n  canon align <corpus-path> --template <name|path> --output <plan.toml> \
         [--frontmatter-output <fm-plan.toml>]\n\n\
         ARGS:\n  <corpus-path>   Directory to align\n\n\
         OPTIONS:\n\
           --template, -t          Template name or explicit path (required)\n\
           --output, -o            Anchor-compatible plan output path (required)\n\
           --frontmatter-output    FM migration plan path (default: <plan>.fm-plan.toml)\n\n\
         EXIT CODES:\n  0  No drift\n  1  Drift found — plan written\n  2  Error"
    );
}

// ---------------------------------------------------------------------------
// Path helpers (Rule 13)
// ---------------------------------------------------------------------------

/// Resolve a src path: workspace-root-relative first, then CWD-relative.
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

/// Resolve a dst path: always CWD-relative for relative inputs (Rule 13).
fn resolve_dst_path(path_str: &str, cwd: &Path) -> PathBuf {
    let p = Path::new(path_str);
    if p.is_absolute() {
        return p.to_owned();
    }
    cwd.join(p)
}

/// Derive the default FM plan path from the main plan path.
/// E.g., `my-plan.toml` → `my-plan.fm-plan.toml`.
fn derive_fm_path(main_plan_path: &Path) -> PathBuf {
    let stem = main_plan_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "plan".to_string());
    let parent = main_plan_path.parent().unwrap_or(Path::new("."));
    parent.join(format!("{}.fm-plan.toml", stem))
}

/// Resolve an explicit --frontmatter-output path or derive from the main plan path.
fn resolve_fm_default(fm_path_str: &str, _main_path: &Path, cwd: &Path) -> PathBuf {
    resolve_dst_path(fm_path_str, cwd)
}

fn is_path_spec(spec: &str) -> bool {
    spec.starts_with('/') || spec.starts_with("./") || spec.starts_with("../")
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
    use tempfile::TempDir;

    #[test]
    fn derive_fm_path_adds_suffix() {
        let main = Path::new("/tmp/corpus-plan.toml");
        let fm = derive_fm_path(main);
        assert_eq!(fm, PathBuf::from("/tmp/corpus-plan.fm-plan.toml"));
    }

    #[test]
    fn align_errors_on_missing_corpus() {
        let dir = TempDir::new().unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_impl(
            "nonexistent-corpus",
            "canon-default",
            "plan.toml",
            None,
            dir.path(),
            dir.path(),
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2);
        let err_str = String::from_utf8_lossy(&err);
        assert!(err_str.contains("not found"), "err={err_str}");
    }

    #[test]
    fn align_errors_on_missing_template() {
        let dir = TempDir::new().unwrap();
        let corpus = dir.path().join("corpus");
        std::fs::create_dir(&corpus).unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_impl(
            corpus.to_str().unwrap(),
            "nonexistent-template",
            "plan.toml",
            None,
            dir.path(),
            dir.path(),
            &mut out,
            &mut err,
        );
        assert_eq!(code, 2);
        let err_str = String::from_utf8_lossy(&err);
        assert!(err_str.contains("not found"), "err={err_str}");
    }

    #[test]
    fn align_clean_corpus_exit_0() {
        // A corpus with no drift should exit 0 and write empty plans.
        let dir = TempDir::new().unwrap();
        let corpus = dir.path().join("corpus");
        std::fs::create_dir_all(corpus.join("01-identity")).unwrap();
        std::fs::write(corpus.join("01-identity/_INDEX.md"), "").unwrap();

        let tmpl_dir = dir.path().join(".accelmars/canon/templates/test-clean");
        std::fs::create_dir_all(&tmpl_dir).unwrap();
        std::fs::write(
            tmpl_dir.join("manifest.toml"),
            r#"name = "test-clean"
version = "1.0.0"
description = "test"
[folder_rules]
shape = "numbered-tiers"
"#,
        )
        .unwrap();

        let plan_path = dir.path().join("out.toml");
        let mut out = Vec::new();
        let mut err_buf = Vec::new();
        let code = run_impl(
            corpus.to_str().unwrap(),
            "test-clean",
            plan_path.to_str().unwrap(),
            None,
            dir.path(),
            dir.path(),
            &mut out,
            &mut err_buf,
        );
        let err_str = String::from_utf8_lossy(&err_buf);
        assert_eq!(code, 0, "err: {err_str}");
        assert!(plan_path.is_file(), "plan not written");
        let plan_content = std::fs::read_to_string(&plan_path).unwrap();
        assert!(plan_content.starts_with("version = \"1\""));
        // Empty main plan has no [[ops]].
        assert!(!plan_content.contains("[[ops]]"));
    }
}
