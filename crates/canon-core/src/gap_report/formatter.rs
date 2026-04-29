use std::path::{Path, PathBuf};

use crate::plan::types::{GapReportRow, JudgmentCategory};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum GapReportError {
    Io(std::io::Error),
}

impl std::fmt::Display for GapReportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GapReportError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for GapReportError {}

impl From<std::io::Error> for GapReportError {
    fn from(e: std::io::Error) -> Self {
        GapReportError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// GapReportFormatter
// ---------------------------------------------------------------------------

/// Writes one Markdown file per `GapReportRow` into `<gap_dir>/CANON-NNN-<slug>.md`.
///
/// Naming: scans existing `CANON-NNN-*.md` files in the gap dir at construction
/// time to find the next available NNN. Running twice on the same gap dir
/// continues the numbering sequence (never overwrites).
pub struct GapReportFormatter {
    gap_dir: PathBuf,
    next_n: u32,
}

impl GapReportFormatter {
    /// Build a formatter for the given gap directory.
    ///
    /// Scans existing `CANON-NNN-*.md` files to determine the starting number.
    pub fn new(gap_dir: &Path) -> Self {
        let next_n = scan_next_number(gap_dir);
        Self {
            gap_dir: gap_dir.to_owned(),
            next_n,
        }
    }

    /// Write one file per gap row. Returns the number of files written.
    pub fn write(&self, rows: &[GapReportRow]) -> Result<usize, GapReportError> {
        std::fs::create_dir_all(&self.gap_dir)?;
        for (i, row) in rows.iter().enumerate() {
            let n = self.next_n + i as u32;
            let slug = make_slug(&row.description);
            let filename = format!("CANON-{:03}-{}.md", n, slug);
            let file_path = self.gap_dir.join(&filename);
            let content = render_gap_file(row, n);
            std::fs::write(&file_path, &content)?;
        }
        Ok(rows.len())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Scan `gap_dir` for `CANON-NNN-*.md` files and return the next available NNN.
fn scan_next_number(gap_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(gap_dir) else {
        return 1;
    };
    let mut max_n: u32 = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(n) = parse_canon_number(&name_str) {
            if n > max_n {
                max_n = n;
            }
        }
    }
    max_n + 1
}

/// Parse the NNN from a filename like `CANON-042-some-slug.md`.
fn parse_canon_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("CANON-")?;
    let end = rest.find('-').unwrap_or(rest.len());
    let num_str = &rest[..end];
    num_str.parse::<u32>().ok()
}

/// Build a filesystem-safe slug from a description.
fn make_slug(description: &str) -> String {
    let slug: String = description
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.len() > 40 { &slug[..40] } else { &slug };
    slug.trim_matches('-').to_string()
}

/// Map `JudgmentCategory` to the `template-canonicalization.md` taxonomy string.
fn category_tag(cat: &JudgmentCategory) -> &'static str {
    match cat {
        JudgmentCategory::GraduationBoundary => "graduation",
        JudgmentCategory::TypeAmbiguous => "type-ambiguous",
        JudgmentCategory::EngineClassInference => "engine-class",
        JudgmentCategory::IdAssignment => "id-assignment",
    }
}

fn category_human(cat: &JudgmentCategory) -> &'static str {
    match cat {
        JudgmentCategory::GraduationBoundary => {
            "graduation boundary — where a file should split into a sub-folder"
        }
        JudgmentCategory::TypeAmbiguous => {
            "type-ambiguous — file type cannot be determined from location alone"
        }
        JudgmentCategory::EngineClassInference => {
            "engine-class inference — engine-class field not deterministic from location"
        }
        JudgmentCategory::IdAssignment => {
            "ID assignment — numbered folder ID requires a known-IDs context"
        }
    }
}

fn recommended_action(cat: &JudgmentCategory) -> &'static str {
    match cat {
        JudgmentCategory::GraduationBoundary => {
            "Review the file and decide whether its content warrants splitting into a sub-folder. \
             If yes, create the folder and distribute content; update any inbound references."
        }
        JudgmentCategory::TypeAmbiguous => {
            "Review the file's content and assign the correct `type` frontmatter value. \
             Consult the template's schema for allowed values."
        }
        JudgmentCategory::EngineClassInference => {
            "Determine whether this file belongs to the closed layer (set `boundary: closed`) \
             or should inherit the open default. Update frontmatter accordingly."
        }
        JudgmentCategory::IdAssignment => {
            "Assign the next available numbered prefix for this folder or file. \
             Check existing siblings to avoid ID collisions."
        }
    }
}

fn render_gap_file(row: &GapReportRow, n: u32) -> String {
    let title = format!(
        "gap: {} in {}",
        category_tag(&row.category),
        row.path.display()
    );
    let date = "2026-04-29"; // static for determinism; real executor passes today's date
    format!(
        "---\ntitle: \"{title}\"\ntype: gap\nengine: canon\npriority: medium\n\
         category: {cat}\ncreated: {date}\nsource: canon-template-architecture-CFC-301\n---\n\n\
         ## Case {n:03} — {human}\n\n\
         **File:** `{path}`\n\n\
         **Description:** {desc}\n\n\
         **Why canon could not auto-resolve:** This case requires closed-layer reasoning. \
         Canon's open layer applies only deterministic rules; {reason} falls in the \
         judgment-bearing category where auto-resolution would risk silent data loss.\n\n\
         **Recommended action:** {action}\n",
        cat = category_tag(&row.category),
        human = category_human(&row.category),
        path = row.path.display(),
        desc = row.description,
        reason = category_tag(&row.category),
        action = recommended_action(&row.category),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::JudgmentCategory;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn graduation_row() -> GapReportRow {
        GapReportRow {
            path: PathBuf::from("big-file.md"),
            category: JudgmentCategory::GraduationBoundary,
            description: "file exceeds line threshold".to_string(),
        }
    }

    #[test]
    fn write_produces_file_in_gap_dir() {
        let dir = TempDir::new().unwrap();
        let formatter = GapReportFormatter::new(dir.path());
        let written = formatter.write(&[graduation_row()]).unwrap();
        assert_eq!(written, 1);
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
            .collect();
        assert_eq!(files.len(), 1);
        let content = std::fs::read_to_string(files[0].path()).unwrap();
        assert!(content.contains("type: gap"));
        assert!(content.contains("category: graduation"));
        assert!(content.contains("engine: canon"));
        assert!(content.contains("source: canon-template-architecture-CFC-301"));
    }

    #[test]
    fn write_continues_numbering_across_runs() {
        let dir = TempDir::new().unwrap();
        let formatter1 = GapReportFormatter::new(dir.path());
        formatter1.write(&[graduation_row()]).unwrap(); // writes CANON-001-*.md

        let formatter2 = GapReportFormatter::new(dir.path());
        formatter2.write(&[graduation_row()]).unwrap(); // should write CANON-002-*.md

        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
            .collect();
        assert_eq!(files.len(), 2);

        let mut names: Vec<_> = files
            .iter()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert!(names[0].starts_with("CANON-001"), "names={names:?}");
        assert!(names[1].starts_with("CANON-002"), "names={names:?}");
    }

    #[test]
    fn slug_truncates_long_descriptions() {
        let long = "a".repeat(80);
        let slug = make_slug(&long);
        assert!(slug.len() <= 40, "slug too long: {}", slug.len());
    }

    #[test]
    fn parse_canon_number_extracts_nnn() {
        assert_eq!(parse_canon_number("CANON-042-some-slug.md"), Some(42));
        assert_eq!(parse_canon_number("CANON-001-x.md"), Some(1));
        assert_eq!(parse_canon_number("unrelated.md"), None);
    }
}
