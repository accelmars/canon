use std::path::{Path, PathBuf};

use super::drift::{DriftCategory, DriftEntry};
use crate::template::LoadedTemplate;

/// Files exceeding this line count are flagged as graduation candidates.
const GRADUATION_LINE_THRESHOLD: usize = 500;

/// Files with this many or more H2 sections trigger `ContentSplitSuggested`
/// when `atomic_file_gate = true`.
const H2_SPLIT_THRESHOLD: usize = 4;

/// Run cross-file and per-file invariant checks defined in the template.
///
/// Emits: `GraduationCandidate`, `ContentSplitSuggested`, `InvariantViolation`.
pub fn check_invariants(
    corpus_root: &Path,
    template: &LoadedTemplate,
    all_md_files: &[PathBuf],
) -> Vec<DriftEntry> {
    let mut entries = Vec::new();
    let atomic_gate = template
        .manifest
        .invariants
        .as_ref()
        .is_some_and(|inv| inv.atomic_file_gate);

    // Per-file heuristic checks.
    for file_path in all_md_files {
        let Ok(content) = std::fs::read_to_string(file_path) else {
            continue;
        };

        let line_count = content.lines().count();
        if line_count > GRADUATION_LINE_THRESHOLD {
            entries.push(DriftEntry {
                path: file_path.clone(),
                category: DriftCategory::GraduationCandidate,
                message: format!(
                    "file has {} lines (threshold {}); consider graduating to a sub-folder",
                    line_count, GRADUATION_LINE_THRESHOLD
                ),
            });
        }

        if atomic_gate {
            let h2_count = content.lines().filter(|l| l.starts_with("## ")).count();
            if h2_count >= H2_SPLIT_THRESHOLD {
                entries.push(DriftEntry {
                    path: file_path.clone(),
                    category: DriftCategory::ContentSplitSuggested,
                    message: format!(
                        "file has {} H2 sections (threshold {}); atomic-file-gate suggests splitting",
                        h2_count, H2_SPLIT_THRESHOLD
                    ),
                });
            }
        }
    }

    // Cross-file invariant: gaps_folder must be a directory, not a file.
    //
    // If the template declares a gaps_folder and the corpus root contains a
    // plain FILE with that name (instead of a directory), it is an invariant
    // violation — canon cannot write gap-report files into it.
    if let Some(gf) = template
        .manifest
        .invariants
        .as_ref()
        .and_then(|inv| inv.gaps_folder.as_deref())
    {
        let gaps_path = corpus_root.join(gf);
        if gaps_path.is_file() {
            entries.push(DriftEntry {
                path: gaps_path,
                category: DriftCategory::InvariantViolation,
                message: format!(
                    "gaps_folder '{}' exists as a file, not a directory; \
                     canon cannot write gap reports into it",
                    gf
                ),
            });
        }
    }

    entries
}
