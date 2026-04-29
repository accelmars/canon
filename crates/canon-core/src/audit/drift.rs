use std::path::PathBuf;

/// Classification of a structural or metadata deviation from a template.
///
/// Categories marked "informational" are reported in the output but do NOT
/// contribute to the exit-code "drift" signal (exit 1). All others are blocking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftCategory {
    /// A top-level directory does not conform to the template's folder shape.
    FolderShape,
    /// A numbered folder is missing its required `_INDEX.md`.
    MissingIndex,
    /// A required frontmatter field is absent.
    FrontmatterRequiredMissing,
    /// A frontmatter field has the wrong JSON type.
    FrontmatterTypeWrong,
    /// A frontmatter field's value is not in the allowed enum.
    FrontmatterValueInvalid,
    /// File exceeds the line threshold and could be graduated to a sub-folder (heuristic).
    GraduationCandidate,
    /// File has many H2 sections; atomic-file-gate suggests splitting (heuristic).
    ContentSplitSuggested,
    /// Frontmatter field is present but not defined in the schema (informational).
    UnknownFieldInfo,
    /// A cross-file invariant declared in the template is violated.
    InvariantViolation,
}

impl DriftCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriftCategory::FolderShape => "FolderShape",
            DriftCategory::MissingIndex => "MissingIndex",
            DriftCategory::FrontmatterRequiredMissing => "FrontmatterRequiredMissing",
            DriftCategory::FrontmatterTypeWrong => "FrontmatterTypeWrong",
            DriftCategory::FrontmatterValueInvalid => "FrontmatterValueInvalid",
            DriftCategory::GraduationCandidate => "GraduationCandidate",
            DriftCategory::ContentSplitSuggested => "ContentSplitSuggested",
            DriftCategory::UnknownFieldInfo => "UnknownFieldInfo",
            DriftCategory::InvariantViolation => "InvariantViolation",
        }
    }

    /// Returns true if this category is informational-only (does not affect exit code).
    ///
    /// Heuristic categories (GraduationCandidate, ContentSplitSuggested) are advisory
    /// signals — their names ("Candidate", "Suggested") and code comments already label
    /// them heuristics. They must not block exit 0 on corpora written before the gate
    /// was enforced; real violations (FrontmatterRequiredMissing, FolderShape, etc.) do.
    pub fn is_informational(&self) -> bool {
        matches!(
            self,
            DriftCategory::UnknownFieldInfo
                | DriftCategory::GraduationCandidate
                | DriftCategory::ContentSplitSuggested
        )
    }
}

/// A single deviation found during corpus audit.
#[derive(Debug, Clone)]
pub struct DriftEntry {
    pub path: PathBuf,
    pub category: DriftCategory,
    pub message: String,
}
