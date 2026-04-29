pub mod loader;
pub mod manifest;
pub mod validate;

pub use loader::{ListedTemplate, LoadedTemplate, TemplateLoader, TemplateTier};
pub use manifest::{
    FolderRules, FolderShape, FrontmatterRef, Invariants, NamingConventions, TemplateManifest,
};

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum TemplateError {
    /// Template name was not found in any resolution tier.
    /// The `searched` list names every path that was checked.
    NotFound {
        name: String,
        searched: Vec<PathBuf>,
    },

    /// `manifest.toml` could not be parsed or is structurally invalid.
    Malformed { source: String, error: String },

    /// Template references a schema file that does not exist on disk.
    MissingSchema {
        template: String,
        schema_path: PathBuf,
    },

    /// Filesystem I/O failure.
    Io(std::io::Error),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::NotFound { name, searched } => {
                let paths: Vec<_> = searched.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "template '{}' not found; searched: {}",
                    name,
                    if paths.is_empty() {
                        "(built-in registry only)".to_string()
                    } else {
                        paths.join(", ")
                    }
                )
            }
            TemplateError::Malformed { source, error } => {
                write!(f, "malformed template '{source}': {error}")
            }
            TemplateError::MissingSchema {
                template,
                schema_path,
            } => {
                write!(
                    f,
                    "template '{template}' references schema '{}' which does not exist",
                    schema_path.display()
                )
            }
            TemplateError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TemplateError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TemplateError {
    fn from(e: std::io::Error) -> Self {
        TemplateError::Io(e)
    }
}
