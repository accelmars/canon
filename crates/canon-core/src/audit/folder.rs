use std::path::Path;

use super::drift::{DriftCategory, DriftEntry};
use crate::template::{FolderShape, LoadedTemplate};

/// Returns true when a directory name follows the numbered-tier convention: `\d\d-`.
fn is_numbered_tier(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 3 && b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[2] == b'-'
}

/// Check folder shape conformance for the top-level directories of the corpus.
///
/// For `numbered-tiers`: every top-level subdirectory must be `\d\d-*`; each
/// such directory must also contain `_INDEX.md`.
/// For `flat`: no subdirectories may exist.
/// For `by-domain` / `custom`: no structural enforcement.
pub fn check_folder_shape(corpus_root: &Path, template: &LoadedTemplate) -> Vec<DriftEntry> {
    let mut entries = Vec::new();
    let shape = &template.manifest.folder_rules.shape;

    match shape {
        FolderShape::NumberedTiers => {
            let Ok(dir_entries) = std::fs::read_dir(corpus_root) else {
                return entries;
            };
            for entry in dir_entries.flatten() {
                // Rule 12: file_type() — never path.is_dir() — to avoid symlink following.
                if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !is_numbered_tier(&name) {
                    entries.push(DriftEntry {
                        path: entry.path(),
                        category: DriftCategory::FolderShape,
                        message: format!(
                            "directory '{}' does not follow numbered-tier naming (expected NN-name)",
                            name
                        ),
                    });
                } else {
                    let index = entry.path().join("_INDEX.md");
                    if !index.is_file() {
                        entries.push(DriftEntry {
                            path: entry.path(),
                            category: DriftCategory::MissingIndex,
                            message: format!("numbered folder '{}' is missing _INDEX.md", name),
                        });
                    }
                }
            }
        }
        FolderShape::Flat => {
            let Ok(dir_entries) = std::fs::read_dir(corpus_root) else {
                return entries;
            };
            for entry in dir_entries.flatten() {
                if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    entries.push(DriftEntry {
                        path: entry.path(),
                        category: DriftCategory::FolderShape,
                        message: format!("flat template: subdirectory '{}' should not exist", name),
                    });
                }
            }
        }
        FolderShape::ByDomain | FolderShape::Custom => {
            // No structural enforcement for by-domain or custom shapes.
        }
    }

    entries
}
