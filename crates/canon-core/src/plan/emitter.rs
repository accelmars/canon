use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::Deserialize;

use crate::audit::DriftCategory;
use crate::audit::DriftEntry;
use crate::template::LoadedTemplate;

use super::judgment_iface::JudgmentEmitter;
use super::types::{
    FmPlan, FmPlanOp, GapReportRow, JudgmentCase, JudgmentCategory, MainPlan, MainPlanOp,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Output of a single plan emission pass.
#[derive(Debug)]
pub struct PlanEmission {
    /// Structural anchor plan (create_dir + move ops).
    pub main_plan: MainPlan,
    /// Frontmatter migration plan (add_field + set_field ops).
    pub fm_plan: FmPlan,
    /// Gap report rows produced by the JudgmentEmitter.
    pub gap_rows: Vec<GapReportRow>,
}

/// Errors that can occur during plan emission.
#[derive(Debug)]
pub enum EmitError {
    Io(io::Error),
    Serialize(String),
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::Io(e) => write!(f, "I/O error: {e}"),
            EmitError::Serialize(s) => write!(f, "serialization error: {s}"),
        }
    }
}

impl std::error::Error for EmitError {}

impl From<io::Error> for EmitError {
    fn from(e: io::Error) -> Self {
        EmitError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// MechanicalPlanEmitter
// ---------------------------------------------------------------------------

/// Classifies drift entries into mechanical plan ops and judgment cases.
///
/// Open-layer: all logic is deterministic given the drift report and template.
/// Judgment-bearing decisions (graduation, type ambiguity, engine-class,
/// ID assignment) are delegated to the injected `JudgmentEmitter`.
pub struct MechanicalPlanEmitter<'a> {
    judgment_emitter: &'a dyn JudgmentEmitter,
}

impl<'a> MechanicalPlanEmitter<'a> {
    pub fn new(judgment_emitter: &'a dyn JudgmentEmitter) -> Self {
        Self { judgment_emitter }
    }

    /// Classify drift entries and emit plans + gap rows.
    ///
    /// `corpus_root` is the directory that was audited.
    /// `workspace_root` is the anchor workspace root — plan paths are produced
    /// relative to this directory so that `anchor plan validate` and `anchor apply`
    /// resolve them correctly. If `None`, paths are relative to `corpus_root`'s parent.
    pub fn emit(
        &self,
        corpus_root: &Path,
        drift: &[DriftEntry],
        template: &LoadedTemplate,
    ) -> Result<PlanEmission, EmitError> {
        self.emit_with_root(corpus_root, drift, template, None)
    }

    /// Variant that accepts an explicit workspace root for path resolution.
    pub fn emit_with_root(
        &self,
        corpus_root: &Path,
        drift: &[DriftEntry],
        template: &LoadedTemplate,
        workspace_root: Option<&Path>,
    ) -> Result<PlanEmission, EmitError> {
        // Load folder number map from template's folder-rules.toml if available.
        let folder_map = template
            .dir
            .as_deref()
            .map(load_folder_number_map)
            .unwrap_or_default();

        // The "root" to strip when producing plan paths.
        // Prefer workspace_root; fall back to corpus_root's parent.
        let path_root: &Path = workspace_root.unwrap_or_else(|| {
            corpus_root.parent().unwrap_or(corpus_root)
        });

        let corpus_name = corpus_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut main_plan = MainPlan::new(Some(format!(
            "canon align: structural ops for '{corpus_name}'"
        )));
        let mut fm_plan = FmPlan::new(Some(format!(
            "canon align: frontmatter migrations for '{corpus_name}'"
        )));
        let mut judgment_cases: Vec<JudgmentCase> = Vec::new();

        for entry in drift {
            let rel = strip_prefix_str(path_root, &entry.path);

            match entry.category {
                DriftCategory::FolderShape => {
                    let folder_name = entry
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if let Some(numbered) = folder_map.get(&folder_name) {
                        // Mechanical rename: target number is known from folder-rules.toml.
                        // src is the root-relative path to the current folder.
                        let src = rel
                            .clone()
                            .unwrap_or_else(|| path_display(&entry.path));
                        let parent = entry
                            .path
                            .parent()
                            .and_then(|p| strip_prefix_str(path_root, p))
                            .filter(|prefix| !prefix.is_empty());
                        let dst = match &parent {
                            Some(p) => format!("{}/{}", p, numbered),
                            None => numbered.clone(),
                        };
                        main_plan.ops.push(MainPlanOp::Move { src, dst });
                    } else {
                        // Target number unknown — ID assignment required.
                        judgment_cases.push(JudgmentCase {
                            path: entry.path.clone(),
                            category: JudgmentCategory::IdAssignment,
                            description: format!(
                                "folder '{}' needs a numbered prefix; no mapping found in template folder-rules",
                                folder_name
                            ),
                        });
                    }
                }

                DriftCategory::MissingIndex => {
                    // Creating files is not in the anchor plan format (only create_dir + move).
                    // This requires human authoring — gap report.
                    judgment_cases.push(JudgmentCase {
                        path: entry.path.clone(),
                        category: JudgmentCategory::IdAssignment,
                        description: format!(
                            "_INDEX.md must be authored in '{}'; content authoring not automatable",
                            path_display(&entry.path)
                        ),
                    });
                }

                DriftCategory::FrontmatterRequiredMissing => {
                    if let Some(field) = extract_field_from_message(&entry.message) {
                        let rel_file = rel.unwrap_or_else(|| path_display(&entry.path));
                        fm_plan.ops.push(FmPlanOp::AddField {
                            path: rel_file,
                            field,
                            value: "FIXME".to_string(),
                        });
                    }
                }

                DriftCategory::FrontmatterTypeWrong | DriftCategory::FrontmatterValueInvalid => {
                    if let Some(field) = extract_field_from_message(&entry.message) {
                        let rel_file = rel.unwrap_or_else(|| path_display(&entry.path));
                        fm_plan.ops.push(FmPlanOp::SetField {
                            path: rel_file,
                            field,
                            value: "FIXME".to_string(),
                        });
                    }
                }

                DriftCategory::GraduationCandidate | DriftCategory::ContentSplitSuggested => {
                    judgment_cases.push(JudgmentCase {
                        path: entry.path.clone(),
                        category: JudgmentCategory::GraduationBoundary,
                        description: entry.message.clone(),
                    });
                }

                DriftCategory::UnknownFieldInfo => {
                    // Informational — no action.
                }

                DriftCategory::InvariantViolation => {
                    // Invariant violations may have multiple causes; conservative gap report.
                    judgment_cases.push(JudgmentCase {
                        path: entry.path.clone(),
                        category: JudgmentCategory::TypeAmbiguous,
                        description: entry.message.clone(),
                    });
                }
            }
        }

        let gap_rows = self.judgment_emitter.emit_judgment_cases(&judgment_cases);

        Ok(PlanEmission {
            main_plan,
            fm_plan,
            gap_rows,
        })
    }

    /// Write the main plan to `path` atomically (write to temp, rename into place).
    pub fn write_main_plan(&self, emission: &PlanEmission, path: &Path) -> Result<(), EmitError> {
        write_atomically(path, &emission.main_plan.render_toml())
    }

    /// Write the FM plan to `path` atomically.
    pub fn write_fm_plan(&self, emission: &PlanEmission, path: &Path) -> Result<(), EmitError> {
        write_atomically(path, &emission.fm_plan.render_toml())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Minimal deserializable struct for reading `folder-rules.toml` category registry.
#[derive(Deserialize)]
struct FolderRulesFile {
    #[serde(default)]
    categories: Vec<CategoryEntry>,
}

#[derive(Deserialize)]
struct CategoryEntry {
    folder: String,
}

/// Load the folder number map from `<template_dir>/folder-rules.toml`.
///
/// Returns a map from un-numbered folder name to numbered folder name.
/// E.g., `"analysis"` → `"32-analysis"`.
/// Returns an empty map if the file does not exist or cannot be parsed.
fn load_folder_number_map(template_dir: &Path) -> HashMap<String, String> {
    let path = template_dir.join("folder-rules.toml");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let parsed: FolderRulesFile = match toml::from_str(&content) {
        Ok(f) => f,
        Err(_) => return HashMap::new(),
    };
    let mut map = HashMap::new();
    for cat in parsed.categories {
        // Folder name is like "32-analysis"; strip the "NN-" prefix to get "analysis".
        if let Some(idx) = cat.folder.find('-') {
            let prefix = &cat.folder[..idx];
            if prefix.len() == 2 && prefix.bytes().all(|b| b.is_ascii_digit()) {
                let name_part = cat.folder[idx + 1..].to_string();
                map.insert(name_part, cat.folder);
            }
        }
    }
    map
}

/// Extract the field name from a frontmatter drift message.
///
/// Handles: "required field 'X' is absent", "field 'X' expected...", etc.
fn extract_field_from_message(msg: &str) -> Option<String> {
    let marker = "field '";
    let start = msg.find(marker)? + marker.len();
    let end = msg[start..].find('\'')?;
    Some(msg[start..start + end].to_string())
}

/// Strip `root` from `path`, returning a forward-slash-separated relative string.
fn strip_prefix_str(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Write `content` to `path` via a temp file + rename (atomic on POSIX).
fn write_atomically(path: &Path, content: &str) -> Result<(), EmitError> {
    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write to a sibling temp file, then rename.
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::judgment_iface::DefaultJudgmentEmitter;
    use std::path::PathBuf;

    fn make_entry(path: &str, category: DriftCategory, msg: &str) -> DriftEntry {
        DriftEntry {
            path: PathBuf::from(path),
            category,
            message: msg.to_string(),
        }
    }

    #[test]
    fn empty_drift_produces_empty_plans() {
        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::BuiltIn,
            dir: None,
        };
        let emission = emitter
            .emit(Path::new("/corpus"), &[], &template)
            .unwrap();
        assert!(emission.main_plan.is_empty());
        assert!(emission.fm_plan.is_empty());
        assert!(emission.gap_rows.is_empty());
    }

    #[test]
    fn graduation_candidate_produces_gap_row() {
        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::BuiltIn,
            dir: None,
        };
        let drift = vec![make_entry(
            "/corpus/big-file.md",
            DriftCategory::GraduationCandidate,
            "file exceeds line threshold",
        )];
        let emission = emitter.emit(Path::new("/corpus"), &drift, &template).unwrap();
        assert!(emission.main_plan.is_empty());
        assert_eq!(emission.gap_rows.len(), 1);
        assert_eq!(
            emission.gap_rows[0].category,
            JudgmentCategory::GraduationBoundary
        );
    }

    #[test]
    fn frontmatter_required_missing_produces_fm_op() {
        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::BuiltIn,
            dir: None,
        };
        let drift = vec![make_entry(
            "/corpus/01-identity/_INDEX.md",
            DriftCategory::FrontmatterRequiredMissing,
            "required field 'schema_version' is absent",
        )];
        let emission = emitter.emit(Path::new("/corpus"), &drift, &template).unwrap();
        assert_eq!(emission.fm_plan.ops.len(), 1);
        match &emission.fm_plan.ops[0] {
            FmPlanOp::AddField { field, .. } => assert_eq!(field, "schema_version"),
            other => panic!("expected AddField, got {:?}", other),
        }
    }

    #[test]
    fn unknown_field_info_is_silent() {
        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::BuiltIn,
            dir: None,
        };
        let drift = vec![make_entry(
            "/corpus/foo.md",
            DriftCategory::UnknownFieldInfo,
            "field 'custom_field' is not defined in the frontmatter schema",
        )];
        let emission = emitter.emit(Path::new("/corpus"), &drift, &template).unwrap();
        assert!(emission.main_plan.is_empty());
        assert!(emission.fm_plan.is_empty());
        assert!(emission.gap_rows.is_empty());
    }

    #[test]
    fn extract_field_from_message_required() {
        assert_eq!(
            extract_field_from_message("required field 'schema_version' is absent"),
            Some("schema_version".to_string())
        );
    }

    #[test]
    fn extract_field_from_message_type_wrong() {
        assert_eq!(
            extract_field_from_message("field 'status' expected type 'string' but got 'integer'"),
            Some("status".to_string())
        );
    }

    #[test]
    fn extract_field_from_message_value_invalid() {
        assert_eq!(
            extract_field_from_message("field 'type' value 'UNKNOWN' is not in allowed enum [...]"),
            Some("type".to_string())
        );
    }

    #[test]
    fn folder_number_map_parses_correctly() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let rules = r#"
[[categories]]
number = "32"
folder = "32-analysis"
what = "Research briefs"
layer = "universal"

[[categories]]
number = "01"
folder = "01-identity"
what = "Identity"
layer = "universal"
"#;
        std::fs::write(dir.path().join("folder-rules.toml"), rules).unwrap();
        let map = load_folder_number_map(dir.path());
        assert_eq!(map.get("analysis"), Some(&"32-analysis".to_string()));
        assert_eq!(map.get("identity"), Some(&"01-identity".to_string()));
    }

    #[test]
    fn folder_shape_drift_with_map_produces_move_op() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("folder-rules.toml"),
            "[[categories]]\nnumber = \"32\"\nfolder = \"32-analysis\"\nwhat = \"\"\nlayer = \"universal\"\n",
        )
        .unwrap();

        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let corpus_root = PathBuf::from("/workspace/corpus");
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::Workspace,
            dir: Some(dir.path().to_owned()),
        };
        let drift = vec![DriftEntry {
            path: corpus_root.join("analysis"),
            category: DriftCategory::FolderShape,
            message: "directory 'analysis' does not follow numbered-tier naming".to_string(),
        }];
        let emission = emitter.emit(&corpus_root, &drift, &template).unwrap();
        assert_eq!(emission.main_plan.ops.len(), 1);
        match &emission.main_plan.ops[0] {
            MainPlanOp::Move { src, dst } => {
                assert!(src.ends_with("analysis"), "src={src}");
                assert!(dst.ends_with("32-analysis"), "dst={dst}");
            }
            other => panic!("expected Move, got {:?}", other),
        }
    }

    #[test]
    fn folder_shape_drift_without_map_produces_gap_row() {
        let emitter = MechanicalPlanEmitter::new(&DefaultJudgmentEmitter);
        let template = crate::template::LoadedTemplate {
            manifest: minimal_manifest(),
            tier: crate::template::TemplateTier::BuiltIn,
            dir: None,
        };
        let drift = vec![DriftEntry {
            path: PathBuf::from("/corpus/unknown-folder"),
            category: DriftCategory::FolderShape,
            message: "directory 'unknown-folder' does not follow numbered-tier naming".to_string(),
        }];
        let emission = emitter
            .emit(Path::new("/corpus"), &drift, &template)
            .unwrap();
        assert!(emission.main_plan.is_empty());
        assert_eq!(emission.gap_rows.len(), 1);
        assert_eq!(emission.gap_rows[0].category, JudgmentCategory::IdAssignment);
    }

    fn minimal_manifest() -> crate::template::TemplateManifest {
        use crate::template::manifest::{FolderRules, FolderShape};
        crate::template::TemplateManifest {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            description: "test template".to_string(),
            folder_rules: FolderRules {
                shape: FolderShape::NumberedTiers,
            },
            frontmatter: None,
            invariants: None,
            naming_conventions: None,
        }
    }
}
