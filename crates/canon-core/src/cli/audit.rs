use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::audit::{self, has_blocking_drift, DriftEntry};
use crate::template::{TemplateError, TemplateLoader};

/// Output format for the audit report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Markdown,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            "markdown" => Ok(OutputFormat::Markdown),
            other => Err(format!(
                "unknown format '{}'; valid values: table, json, markdown",
                other
            )),
        }
    }
}

/// Production entry point — resolves workspace root and CWD from the environment.
///
/// Returns exit code: 0 = conformant, 1 = drift found, 2 = error.
pub fn run(
    corpus_path_str: &str,
    template_spec: &str,
    format: &OutputFormat,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let workspace_root = find_workspace_root();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    run_impl(
        corpus_path_str,
        template_spec,
        format,
        &workspace_root,
        &cwd,
        out,
        err,
    )
}

/// Testable implementation — accepts explicit workspace_root, cwd, and output writers.
///
/// Follows Rule 13: no global state (no set_current_dir, no env-var side effects).
pub fn run_impl(
    corpus_path_str: &str,
    template_spec: &str,
    format: &OutputFormat,
    workspace_root: &Path,
    cwd: &Path,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    // Resolve corpus path per Rule 13: try workspace-root-relative, then CWD-relative.
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

    // Run audit.
    let entries = match audit::run_audit(&corpus_path, &template) {
        Ok(e) => e,
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            return 2;
        }
    };

    // Emit output.
    emit_output(&entries, &corpus_path, format, out);

    if has_blocking_drift(&entries) {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Output rendering
// ---------------------------------------------------------------------------

fn emit_output(
    entries: &[DriftEntry],
    corpus_path: &Path,
    format: &OutputFormat,
    out: &mut dyn Write,
) {
    match format {
        OutputFormat::Table => emit_table(entries, corpus_path, out),
        OutputFormat::Json => emit_json(entries, corpus_path, out),
        OutputFormat::Markdown => emit_markdown(entries, corpus_path, out),
    }
}

fn emit_table(entries: &[DriftEntry], corpus_path: &Path, out: &mut dyn Write) {
    if entries.is_empty() {
        let _ = writeln!(
            out,
            "canon audit: no drift found in '{}'",
            corpus_path.display()
        );
        return;
    }
    let blocking: Vec<_> = entries
        .iter()
        .filter(|e| !e.category.is_informational())
        .collect();
    let info: Vec<_> = entries
        .iter()
        .filter(|e| e.category.is_informational())
        .collect();

    if !blocking.is_empty() {
        let _ = writeln!(out, "{:<40} {:<30} Message", "File", "Category");
        let _ = writeln!(out, "{}", "-".repeat(110));
        for entry in &blocking {
            let rel = entry.path.strip_prefix(corpus_path).unwrap_or(&entry.path);
            let _ = writeln!(
                out,
                "{:<40} {:<30} {}",
                rel.display(),
                entry.category.as_str(),
                entry.message
            );
        }
        let _ = writeln!(out, "\n{} blocking drift entries.", blocking.len());
    }
    if !info.is_empty() {
        let _ = writeln!(out, "\n[info] {} informational notices:", info.len());
        for entry in &info {
            let rel = entry.path.strip_prefix(corpus_path).unwrap_or(&entry.path);
            let _ = writeln!(
                out,
                "  {} — {} — {}",
                rel.display(),
                entry.category.as_str(),
                entry.message
            );
        }
    }
}

fn emit_json(entries: &[DriftEntry], corpus_path: &Path, out: &mut dyn Write) {
    let json_entries: Vec<_> = entries
        .iter()
        .map(|e| {
            let rel = e.path.strip_prefix(corpus_path).unwrap_or(&e.path);
            json!({
                "path": rel.to_string_lossy(),
                "category": e.category.as_str(),
                "informational": e.category.is_informational(),
                "message": e.message,
            })
        })
        .collect();

    let blocking_count = entries
        .iter()
        .filter(|e| !e.category.is_informational())
        .count();
    let output = json!({
        "corpus": corpus_path.to_string_lossy(),
        "blocking_count": blocking_count,
        "total_count": entries.len(),
        "entries": json_entries,
    });

    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
    );
}

fn emit_markdown(entries: &[DriftEntry], corpus_path: &Path, out: &mut dyn Write) {
    let blocking_count = entries
        .iter()
        .filter(|e| !e.category.is_informational())
        .count();
    if blocking_count == 0 {
        let _ = writeln!(
            out,
            "## Canon Audit — No Drift Found\n\nCorpus: `{}`\n",
            corpus_path.display()
        );
        return;
    }
    let _ = writeln!(out, "## Canon Audit — {} Drift Entries\n", blocking_count);
    let _ = writeln!(out, "Corpus: `{}`\n", corpus_path.display());
    let _ = writeln!(out, "| File | Category | Message |");
    let _ = writeln!(out, "|------|----------|---------|");
    for entry in entries.iter().filter(|e| !e.category.is_informational()) {
        let rel = entry.path.strip_prefix(corpus_path).unwrap_or(&entry.path);
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            rel.display(),
            entry.category.as_str(),
            entry.message.replace('|', "\\|")
        );
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Resolve a src path per Rule 13: workspace-root-relative first, then CWD-relative.
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

/// Returns true when the template spec looks like a filesystem path rather than a name.
fn is_path_spec(spec: &str) -> bool {
    spec.starts_with('/') || spec.starts_with("./") || spec.starts_with("../")
}

/// Walk parent directories from CWD looking for `.accelmars/` to find the workspace root.
/// Falls back to CWD if not found.
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
