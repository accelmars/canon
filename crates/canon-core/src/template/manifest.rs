use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TemplateManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub folder_rules: FolderRules,
    pub frontmatter: Option<FrontmatterRef>,
    pub invariants: Option<Invariants>,
    pub naming_conventions: Option<NamingConventions>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FolderRules {
    pub shape: FolderShape,
}

/// Declared output shape for a structure template.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FolderShape {
    /// Numbered top-level folders (e.g. 10-intake/, 20-foundations/).
    NumberedTiers,
    /// All files in a single directory with no subfolder hierarchy.
    Flat,
    /// Files grouped into domain-named subdirectories.
    ByDomain,
    /// Escape hatch: unrecognized structure; canon reports all drift as gap rows.
    Custom,
}

/// Reference to a JSON Schema 2020-12 file for frontmatter validation.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FrontmatterRef {
    /// Path relative to the template directory. Absolute paths and URLs are rejected.
    pub schema: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Invariants {
    #[serde(default)]
    pub index_required: bool,
    pub gaps_folder: Option<String>,
    #[serde(default)]
    pub atomic_file_gate: bool,
}

/// Reserved for future naming-convention rules. Currently parsed but not enforced.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
pub struct NamingConventions {}
