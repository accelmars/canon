use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::manifest::TemplateManifest;
use super::TemplateError;

/// Which resolution tier a template was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateTier {
    BuiltIn,
    Workspace,
    User,
    ExplicitPath(PathBuf),
}

/// A successfully loaded template with its resolved manifest and source metadata.
#[derive(Debug, Clone)]
pub struct LoadedTemplate {
    pub manifest: TemplateManifest,
    pub tier: TemplateTier,
    /// Directory on disk for filesystem-based templates; None for built-ins.
    pub dir: Option<PathBuf>,
}

/// A single entry returned by `list_all`.
#[derive(Debug, Clone)]
pub struct ListedTemplate {
    pub name: String,
    pub tier: TemplateTier,
    pub description: String,
}

/// Built-in templates compiled into the binary as static strings.
static COMPILED_BUILT_INS: &[(&str, &str)] = &[(
    "canon-default",
    include_str!("../../templates/canon-default/manifest.toml"),
)];

/// Returns the production built-in registry as an owned Vec.
/// Used in tests to create a loader with production built-ins and controlled paths.
pub fn production_builtins() -> Vec<(String, String)> {
    COMPILED_BUILT_INS
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect()
}

pub struct TemplateLoader {
    builtins: Vec<(String, String)>,
    workspace_templates_dir: PathBuf,
    user_templates_dir: PathBuf,
}

impl TemplateLoader {
    /// Production constructor: resolves workspace and user dirs from the workspace root.
    pub fn from_workspace_root(workspace_root: &Path) -> Self {
        Self {
            builtins: COMPILED_BUILT_INS
                .iter()
                .map(|(n, t)| (n.to_string(), t.to_string()))
                .collect(),
            workspace_templates_dir: workspace_root.join(".accelmars/canon/templates"),
            user_templates_dir: default_user_templates_dir(),
        }
    }

    /// Test-friendly constructor: accepts explicit paths and a built-in registry.
    /// Follows Rule 13 — no global state, no env-var side effects.
    pub fn with_builtins(
        builtins: Vec<(String, String)>,
        workspace_templates_dir: PathBuf,
        user_templates_dir: PathBuf,
    ) -> Self {
        Self {
            builtins,
            workspace_templates_dir,
            user_templates_dir,
        }
    }

    /// Load a template by name.
    ///
    /// Resolution order (first match wins): workspace > user > built-in.
    /// A workspace template with the same name as a built-in overrides the built-in.
    pub fn load_by_name(&self, name: &str) -> Result<LoadedTemplate, TemplateError> {
        let mut searched = Vec::new();

        let ws_dir = self.workspace_templates_dir.join(name);
        if ws_dir.is_dir() {
            return load_from_dir(&ws_dir, TemplateTier::Workspace);
        }
        searched.push(ws_dir);

        let user_dir = self.user_templates_dir.join(name);
        if user_dir.is_dir() {
            return load_from_dir(&user_dir, TemplateTier::User);
        }
        searched.push(user_dir);

        if let Some((_, manifest_toml)) = self.builtins.iter().find(|(n, _)| n == name) {
            let manifest = parse_manifest(manifest_toml, name)?;
            return Ok(LoadedTemplate {
                manifest,
                tier: TemplateTier::BuiltIn,
                dir: None,
            });
        }

        Err(TemplateError::NotFound {
            name: name.to_string(),
            searched,
        })
    }

    /// Load a template from an explicit filesystem path. Bypasses name resolution.
    pub fn load_by_path(&self, path: &Path) -> Result<LoadedTemplate, TemplateError> {
        load_from_dir(path, TemplateTier::ExplicitPath(path.to_owned()))
    }

    /// List all templates from all tiers, each tagged with its source tier.
    ///
    /// Built-ins that share a name with a workspace or user template are suppressed
    /// (the higher-tier template is shown instead).
    pub fn list_all(&self) -> Vec<ListedTemplate> {
        let mut out = Vec::new();
        collect_dir_templates(
            &self.workspace_templates_dir,
            TemplateTier::Workspace,
            &mut out,
        );
        collect_dir_templates(&self.user_templates_dir, TemplateTier::User, &mut out);

        let higher_tier_names: HashSet<String> = out.iter().map(|t| t.name.clone()).collect();
        for (name, manifest_toml) in &self.builtins {
            if !higher_tier_names.contains(name.as_str()) {
                if let Ok(m) = parse_manifest(manifest_toml, name) {
                    out.push(ListedTemplate {
                        name: name.clone(),
                        tier: TemplateTier::BuiltIn,
                        description: m.description,
                    });
                }
            }
        }

        out
    }
}

fn load_from_dir(dir: &Path, tier: TemplateTier) -> Result<LoadedTemplate, TemplateError> {
    let manifest_path = dir.join("manifest.toml");
    let content = fs::read_to_string(&manifest_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            TemplateError::Malformed {
                source: dir.display().to_string(),
                error: "missing manifest.toml".to_string(),
            }
        } else {
            TemplateError::Io(e)
        }
    })?;
    let manifest = parse_manifest(&content, &dir.display().to_string())?;
    Ok(LoadedTemplate {
        manifest,
        tier,
        dir: Some(dir.to_owned()),
    })
}

pub(super) fn parse_manifest(
    content: &str,
    source: &str,
) -> Result<TemplateManifest, TemplateError> {
    toml::from_str(content).map_err(|e| TemplateError::Malformed {
        source: source.to_string(),
        error: e.to_string(),
    })
}

fn collect_dir_templates(dir: &Path, tier: TemplateTier, out: &mut Vec<ListedTemplate>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // Rule 12: use file_type(), not path.is_dir(), to avoid following symlinks.
        if entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let manifest_path = path.join("manifest.toml");
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(m) = parse_manifest(&content, &name) {
                    out.push(ListedTemplate {
                        name,
                        tier: tier.clone(),
                        description: m.description,
                    });
                }
            }
        }
    }
}

fn default_user_templates_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/canon/templates");
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join("canon/templates");
    }
    PathBuf::from(".config/canon/templates")
}
