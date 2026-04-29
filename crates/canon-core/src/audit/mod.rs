pub mod drift;
pub mod folder;
pub mod frontmatter;
pub mod invariants;

use std::path::{Path, PathBuf};

use serde_json::Value as Json;
use walkdir::WalkDir;

use crate::template::{LoadedTemplate, TemplateError};
pub use drift::{DriftCategory, DriftEntry};

/// Errors that can occur during a corpus audit.
#[derive(Debug)]
pub enum AuditError {
    TemplateError(TemplateError),
    MissingSchema(PathBuf),
    MalformedSchema(String),
    Io(std::io::Error),
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditError::TemplateError(e) => write!(f, "template error: {}", e),
            AuditError::MissingSchema(p) => {
                write!(f, "schema file not found: {}", p.display())
            }
            AuditError::MalformedSchema(e) => write!(f, "malformed JSON schema: {}", e),
            AuditError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl std::error::Error for AuditError {}

impl From<std::io::Error> for AuditError {
    fn from(e: std::io::Error) -> Self {
        AuditError::Io(e)
    }
}

/// Audit a corpus directory against a loaded template.
///
/// Returns all `DriftEntry` items found. The caller decides exit code based on
/// whether any entries have `!is_informational()`.
pub fn run_audit(
    corpus_root: &Path,
    template: &LoadedTemplate,
) -> Result<Vec<DriftEntry>, AuditError> {
    let mut entries = Vec::new();

    // Folder shape and _INDEX.md checks.
    entries.extend(folder::check_folder_shape(corpus_root, template));

    // Collect all .md files (respects Rule 12 via walkdir's DirEntry).
    let md_files = collect_md_files(corpus_root);

    // Load frontmatter schema (None for built-ins with no on-disk dir).
    let schema = load_schema(template)?;

    // Per-file frontmatter validation.
    for file_path in &md_files {
        let Ok(content) = std::fs::read_to_string(file_path) else {
            continue;
        };
        entries.extend(frontmatter::check_frontmatter(
            file_path,
            &content,
            schema.as_ref(),
        ));
    }

    // Cross-file and heuristic invariant checks.
    entries.extend(invariants::check_invariants(
        corpus_root,
        template,
        &md_files,
    ));

    Ok(entries)
}

/// Returns true when the drift list contains at least one blocking (non-informational) entry.
pub fn has_blocking_drift(entries: &[DriftEntry]) -> bool {
    entries.iter().any(|e| !e.category.is_informational())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn collect_md_files(corpus_root: &Path) -> Vec<PathBuf> {
    WalkDir::new(corpus_root)
        .follow_links(false) // Rule 12: don't follow symlinks.
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().and_then(|s| s.to_str()) == Some("md")
                && !e.path().components().any(|c| c.as_os_str() == ".git")
        })
        .map(|e| e.path().to_owned())
        .collect()
}

fn load_schema(template: &LoadedTemplate) -> Result<Option<Json>, AuditError> {
    let Some(fm_ref) = &template.manifest.frontmatter else {
        return Ok(None);
    };
    let Some(dir) = &template.dir else {
        // Built-in with no on-disk dir — schema path is unresolvable.
        return Ok(None);
    };
    let schema_path = dir.join(&fm_ref.schema);
    if !schema_path.is_file() {
        return Err(AuditError::MissingSchema(schema_path));
    }
    let content = std::fs::read_to_string(&schema_path)?;
    let schema: Json =
        serde_json::from_str(&content).map_err(|e| AuditError::MalformedSchema(e.to_string()))?;
    Ok(Some(schema))
}
