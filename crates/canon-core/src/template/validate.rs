use super::loader::LoadedTemplate;
use super::TemplateError;

/// Validate a loaded template for internal consistency.
///
/// Checks:
/// - Referenced schema file exists on disk (filesystem-based templates only).
/// - `invariants.gaps_folder` is non-empty if specified.
pub fn validate(template: &LoadedTemplate) -> Result<(), TemplateError> {
    if let Some(ref frontmatter) = template.manifest.frontmatter {
        if let Some(ref dir) = template.dir {
            let schema_path = dir.join(&frontmatter.schema);
            if !schema_path.exists() {
                return Err(TemplateError::MissingSchema {
                    template: template.manifest.name.clone(),
                    schema_path,
                });
            }
        }
    }

    if let Some(ref inv) = template.manifest.invariants {
        if let Some(ref gf) = inv.gaps_folder {
            if gf.is_empty() {
                return Err(TemplateError::Malformed {
                    source: template.manifest.name.clone(),
                    error: "invariants.gaps_folder must not be empty if specified".to_string(),
                });
            }
        }
    }

    Ok(())
}
