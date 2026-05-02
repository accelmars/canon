use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Main plan (anchor structural ops — create_dir / move)
// ---------------------------------------------------------------------------

/// A structural operation in the main anchor plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainPlanOp {
    CreateDir { path: String },
    Move { src: String, dst: String },
}

/// Structural anchor plan emitted as anchor-compatible TOML (version = "1").
#[derive(Debug, Clone, PartialEq)]
pub struct MainPlan {
    pub description: Option<String>,
    pub ops: Vec<MainPlanOp>,
}

impl MainPlan {
    pub fn new(description: Option<String>) -> Self {
        Self {
            description,
            ops: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Render to anchor-compatible TOML (version = "1").
    ///
    /// The format is deterministic: version header, optional description, then
    /// `[[ops]]` blocks in declaration order. Identical inputs always produce
    /// byte-identical output.
    pub fn render_toml(&self) -> String {
        let mut out = String::from("version = \"1\"\n");
        if let Some(d) = &self.description {
            out.push_str(&format!("description = {}\n", toml_string(d)));
        }
        if self.ops.is_empty() {
            out.push_str("ops = []\n");
        } else {
            for op in &self.ops {
                out.push('\n');
                out.push_str("[[ops]]\n");
                match op {
                    MainPlanOp::CreateDir { path } => {
                        out.push_str("type = \"create_dir\"\n");
                        out.push_str(&format!("path = {}\n", toml_string(path)));
                    }
                    MainPlanOp::Move { src, dst } => {
                        out.push_str("type = \"move\"\n");
                        out.push_str(&format!("src = {}\n", toml_string(src)));
                        out.push_str(&format!("dst = {}\n", toml_string(dst)));
                    }
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// FM plan (frontmatter migration ops — consumed by `anchor frontmatter migrate`, AENG-006)
// ---------------------------------------------------------------------------

/// A frontmatter migration operation.
///
/// `add_field` and `set_field` are the v1 op types for `anchor frontmatter migrate`
/// (AENG-006 defines the executor). This format is provisional and will be confirmed
/// when AENG-006 ships in the sibling project `anchor-canonicalization-ready`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmPlanOp {
    /// Add a frontmatter field that does not yet exist.
    AddField {
        path: String,
        field: String,
        value: String,
    },
    /// Overwrite an existing frontmatter field with a corrected value.
    SetField {
        path: String,
        field: String,
        value: String,
    },
}

/// Frontmatter migration plan emitted as TOML for `anchor frontmatter migrate`.
#[derive(Debug, Clone, PartialEq)]
pub struct FmPlan {
    pub description: Option<String>,
    pub ops: Vec<FmPlanOp>,
}

impl FmPlan {
    pub fn new(description: Option<String>) -> Self {
        Self {
            description,
            ops: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Render to provisional FM plan TOML.
    pub fn render_toml(&self) -> String {
        let mut out = String::from("version = \"1\"\n");
        if let Some(d) = &self.description {
            out.push_str(&format!("description = {}\n", toml_string(d)));
        }
        for op in &self.ops {
            out.push('\n');
            out.push_str("[[ops]]\n");
            match op {
                FmPlanOp::AddField { path, field, value } => {
                    out.push_str("type = \"add_field\"\n");
                    out.push_str(&format!("path = {}\n", toml_string(path)));
                    out.push_str(&format!("field = {}\n", toml_string(field)));
                    out.push_str(&format!("value = {}\n", toml_string(value)));
                }
                FmPlanOp::SetField { path, field, value } => {
                    out.push_str("type = \"set_field\"\n");
                    out.push_str(&format!("path = {}\n", toml_string(path)));
                    out.push_str(&format!("field = {}\n", toml_string(field)));
                    out.push_str(&format!("value = {}\n", toml_string(value)));
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Judgment types
// ---------------------------------------------------------------------------

/// Categories of cases that require closed-layer reasoning to resolve.
///
/// Per `template-canonicalization.md` §"What's hard":
/// - `GraduationBoundary` — where a file should split into a folder
/// - `TypeAmbiguous` — file type cannot be determined from location alone
/// - `EngineClassInference` — engine-class field (e.g. `boundary:`) not deterministic
/// - `IdAssignment` — numbered folder ID cannot be assigned without context
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgmentCategory {
    GraduationBoundary,
    TypeAmbiguous,
    EngineClassInference,
    IdAssignment,
}

impl JudgmentCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            JudgmentCategory::GraduationBoundary => "GraduationBoundary",
            JudgmentCategory::TypeAmbiguous => "TypeAmbiguous",
            JudgmentCategory::EngineClassInference => "EngineClassInference",
            JudgmentCategory::IdAssignment => "IdAssignment",
        }
    }
}

/// A case that requires closed-layer judgment to resolve.
#[derive(Debug, Clone)]
pub struct JudgmentCase {
    pub path: PathBuf,
    pub category: JudgmentCategory,
    pub description: String,
}

/// A row in the gap report produced by a `JudgmentEmitter`.
#[derive(Debug, Clone)]
pub struct GapReportRow {
    pub path: PathBuf,
    pub category: JudgmentCategory,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Internal TOML rendering helpers
// ---------------------------------------------------------------------------

fn toml_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_plan_renders_empty() {
        let plan = MainPlan::new(None);
        assert_eq!(plan.render_toml(), "version = \"1\"\nops = []\n");
    }

    #[test]
    fn main_plan_create_dir_roundtrip() {
        let mut plan = MainPlan::new(Some("test plan".to_string()));
        plan.ops.push(MainPlanOp::CreateDir {
            path: "01-identity".to_string(),
        });
        let toml = plan.render_toml();
        assert!(toml.contains("type = \"create_dir\""));
        assert!(toml.contains("path = \"01-identity\""));
        assert!(toml.contains("description = \"test plan\""));
    }

    #[test]
    fn main_plan_move_roundtrip() {
        let mut plan = MainPlan::new(None);
        plan.ops.push(MainPlanOp::Move {
            src: "analysis".to_string(),
            dst: "32-analysis".to_string(),
        });
        let toml = plan.render_toml();
        assert!(toml.contains("type = \"move\""));
        assert!(toml.contains("src = \"analysis\""));
        assert!(toml.contains("dst = \"32-analysis\""));
    }

    #[test]
    fn main_plan_deterministic() {
        let mut plan = MainPlan::new(Some("determinism check".to_string()));
        plan.ops.push(MainPlanOp::Move {
            src: "analysis".to_string(),
            dst: "32-analysis".to_string(),
        });
        plan.ops.push(MainPlanOp::CreateDir {
            path: "41-gaps".to_string(),
        });
        assert_eq!(plan.render_toml(), plan.render_toml());
    }

    #[test]
    fn fm_plan_renders_empty() {
        let plan = FmPlan::new(None);
        assert_eq!(plan.render_toml(), "version = \"1\"\n");
    }

    #[test]
    fn fm_plan_add_field_roundtrip() {
        let mut plan = FmPlan::new(None);
        plan.ops.push(FmPlanOp::AddField {
            path: "01-identity/_INDEX.md".to_string(),
            field: "schema_version".to_string(),
            value: "1".to_string(),
        });
        let toml = plan.render_toml();
        assert!(toml.contains("type = \"add_field\""));
        assert!(toml.contains("field = \"schema_version\""));
        assert!(toml.contains("value = \"1\""));
    }

    #[test]
    fn fm_plan_set_field_roundtrip() {
        let mut plan = FmPlan::new(None);
        plan.ops.push(FmPlanOp::SetField {
            path: "analysis/deep-dive.md".to_string(),
            field: "status".to_string(),
            value: "active".to_string(),
        });
        let toml = plan.render_toml();
        assert!(toml.contains("type = \"set_field\""));
        assert!(toml.contains("field = \"status\""));
    }
}
