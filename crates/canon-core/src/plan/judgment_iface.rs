use super::types::{GapReportRow, JudgmentCase};

/// Closed-layer seam for judgment-bearing canonicalization decisions.
///
/// v1: all implementations produce gap-report rows — no AI auto-resolution.
/// Future: closed implementations may auto-resolve cases where context is sufficient.
///
/// The trait lives in `canon-core` (open). The closed-layer implementation lives in
/// `canon-engine::canon_judgment`. The binary wires them via dependency injection.
pub trait JudgmentEmitter: Send + Sync {
    fn emit_judgment_cases(&self, cases: &[JudgmentCase]) -> Vec<GapReportRow>;
}

/// Default open-layer emitter: maps each judgment case to a gap report row.
///
/// Used by the open binary. Zero AI calls, zero closed-layer dependencies.
pub struct DefaultJudgmentEmitter;

impl JudgmentEmitter for DefaultJudgmentEmitter {
    fn emit_judgment_cases(&self, cases: &[JudgmentCase]) -> Vec<GapReportRow> {
        cases
            .iter()
            .map(|c| GapReportRow {
                path: c.path.clone(),
                category: c.category.clone(),
                description: c.description.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::JudgmentCategory;
    use std::path::PathBuf;

    #[test]
    fn default_emitter_passthrough() {
        let cases = vec![
            JudgmentCase {
                path: PathBuf::from("analysis"),
                category: JudgmentCategory::IdAssignment,
                description: "folder needs numbered prefix".to_string(),
            },
            JudgmentCase {
                path: PathBuf::from("big-file.md"),
                category: JudgmentCategory::GraduationBoundary,
                description: "file exceeds line threshold".to_string(),
            },
        ];
        let emitter = DefaultJudgmentEmitter;
        let rows = emitter.emit_judgment_cases(&cases);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, PathBuf::from("analysis"));
        assert_eq!(rows[0].category, JudgmentCategory::IdAssignment);
        assert_eq!(rows[1].category, JudgmentCategory::GraduationBoundary);
    }

    #[test]
    fn default_emitter_empty_input() {
        let emitter = DefaultJudgmentEmitter;
        let rows = emitter.emit_judgment_cases(&[]);
        assert!(rows.is_empty());
    }
}
