pub mod anchor_runner;

use std::path::PathBuf;

use crate::audit;
use crate::gap_report::formatter::GapReportFormatter;
use crate::plan::{DefaultJudgmentEmitter, MechanicalPlanEmitter};
use crate::template::LoadedTemplate;

use anchor_runner::AnchorRunner;

// ---------------------------------------------------------------------------
// Pipeline step outcomes
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StepOutcome {
    Audit { drift_count: usize, blocking: bool },
    Plan { main_ops: usize, fm_ops: usize, gap_rows: usize },
    ApplySkipped,
    AnchorApply { main_plan_path: PathBuf },
    AnchorApplyFailed { exit_code: i32, diagnostic: String },
    AnchorFmMigrate { fm_plan_path: PathBuf },
    AnchorFmMigrateFailed { exit_code: i32, diagnostic: String },
    GapReport { files_written: usize, gap_dir: PathBuf },
    ReAudit { residual_drift: usize },
}

// ---------------------------------------------------------------------------
// OrchestratorOutcome — typed result enum (R9)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum OrchestratorOutcome {
    Complete {
        steps: Vec<StepOutcome>,
        exit_code: i32,
    },
    Failed {
        steps: Vec<StepOutcome>,
        step_name: String,
        error: String,
    },
}

impl OrchestratorOutcome {
    pub fn exit_code(&self) -> i32 {
        match self {
            OrchestratorOutcome::Complete { exit_code, .. } => *exit_code,
            OrchestratorOutcome::Failed { .. } => 2,
        }
    }

    pub fn steps(&self) -> &[StepOutcome] {
        match self {
            OrchestratorOutcome::Complete { steps, .. } => steps,
            OrchestratorOutcome::Failed { steps, .. } => steps,
        }
    }
}

// ---------------------------------------------------------------------------
// OrchestratorConfig
// ---------------------------------------------------------------------------

pub struct OrchestratorConfig {
    pub corpus_path: PathBuf,
    pub template: LoadedTemplate,
    pub apply: bool,
    pub gap_report_dir: PathBuf,
    pub workspace_root: PathBuf,
}

// ---------------------------------------------------------------------------
// Pipeline helpers
// ---------------------------------------------------------------------------

/// Monotonic counter for unique temp file names — safe across parallel test threads.
static PLAN_INVOCATION: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Generate unique temp file paths per invocation so parallel test runs don't collide.
fn unique_tmp_plan_paths() -> (std::path::PathBuf, std::path::PathBuf) {
    let n = PLAN_INVOCATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let pid = std::process::id();
    let tag = format!("{pid}-{n}");
    let dir = std::env::temp_dir();
    (
        dir.join(format!("canon-main-{tag}.toml")),
        dir.join(format!("canon-fm-{tag}.toml")),
    )
}

// ---------------------------------------------------------------------------
// Pipeline entry point
// ---------------------------------------------------------------------------

/// Run the orchestrator pipeline: Audit → Plan → (Apply | Skip) → GapReport → ReAudit.
///
/// In dry-run mode (`config.apply == false`), nothing is written to disk.
/// In apply mode, anchor is invoked via `runner`, gap files are written, and a
/// re-audit checks residual drift.
pub fn run_pipeline(config: &OrchestratorConfig, runner: &dyn AnchorRunner) -> OrchestratorOutcome {
    let mut steps: Vec<StepOutcome> = Vec::new();

    // ------------------------------------------------------------------
    // Step 1: Audit
    // ------------------------------------------------------------------
    let drift = match audit::run_audit(&config.corpus_path, &config.template) {
        Ok(d) => d,
        Err(e) => {
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "audit".to_string(),
                error: e.to_string(),
            };
        }
    };
    let blocking = audit::has_blocking_drift(&drift);
    let drift_count = drift.iter().filter(|e| !e.category.is_informational()).count();
    steps.push(StepOutcome::Audit { drift_count, blocking });

    // ------------------------------------------------------------------
    // Step 2: Plan emit
    // ------------------------------------------------------------------
    let judgment_emitter = DefaultJudgmentEmitter;
    let plan_emitter = MechanicalPlanEmitter::new(&judgment_emitter);
    let emission = match plan_emitter.emit_with_root(
        &config.corpus_path,
        &drift,
        &config.template,
        Some(&config.workspace_root),
    ) {
        Ok(e) => e,
        Err(e) => {
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "plan-emit".to_string(),
                error: e.to_string(),
            };
        }
    };

    steps.push(StepOutcome::Plan {
        main_ops: emission.main_plan.ops.len(),
        fm_ops: emission.fm_plan.ops.len(),
        gap_rows: emission.gap_rows.len(),
    });

    // ------------------------------------------------------------------
    // Dry-run: no files written
    // ------------------------------------------------------------------
    if !config.apply {
        steps.push(StepOutcome::ApplySkipped);
        let exit_code = if blocking { 1 } else { 0 };
        return OrchestratorOutcome::Complete { steps, exit_code };
    }

    // ------------------------------------------------------------------
    // Apply mode: write plans to temp files (unique per invocation)
    // ------------------------------------------------------------------
    let (main_plan_path, fm_plan_path) = unique_tmp_plan_paths();

    if let Err(e) = plan_emitter.write_main_plan(&emission, &main_plan_path) {
        return OrchestratorOutcome::Failed {
            steps,
            step_name: "write-main-plan".to_string(),
            error: e.to_string(),
        };
    }
    if let Err(e) = plan_emitter.write_fm_plan(&emission, &fm_plan_path) {
        return OrchestratorOutcome::Failed {
            steps,
            step_name: "write-fm-plan".to_string(),
            error: e.to_string(),
        };
    }

    // ------------------------------------------------------------------
    // anchor apply
    // ------------------------------------------------------------------
    match runner.run_apply(&main_plan_path) {
        Ok(()) => {
            steps.push(StepOutcome::AnchorApply {
                main_plan_path: main_plan_path.clone(),
            });
        }
        Err(e) => {
            steps.push(StepOutcome::AnchorApplyFailed {
                exit_code: e.exit_code,
                diagnostic: e.diagnostic.clone(),
            });
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "anchor-apply".to_string(),
                error: format!("anchor apply exited {} — {}", e.exit_code, e.diagnostic),
            };
        }
    }

    // ------------------------------------------------------------------
    // anchor frontmatter migrate
    // ------------------------------------------------------------------
    match runner.run_frontmatter_migrate(&fm_plan_path) {
        Ok(()) => {
            steps.push(StepOutcome::AnchorFmMigrate {
                fm_plan_path: fm_plan_path.clone(),
            });
        }
        Err(e) => {
            steps.push(StepOutcome::AnchorFmMigrateFailed {
                exit_code: e.exit_code,
                diagnostic: e.diagnostic.clone(),
            });
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "anchor-fm-migrate".to_string(),
                error: format!("anchor frontmatter migrate exited {} — {}", e.exit_code, e.diagnostic),
            };
        }
    }

    // ------------------------------------------------------------------
    // Write gap report files
    // ------------------------------------------------------------------
    let formatter = GapReportFormatter::new(&config.gap_report_dir);
    match formatter.write(&emission.gap_rows) {
        Ok(written) => {
            steps.push(StepOutcome::GapReport {
                files_written: written,
                gap_dir: config.gap_report_dir.clone(),
            });
        }
        Err(e) => {
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "gap-report".to_string(),
                error: e.to_string(),
            };
        }
    }

    // ------------------------------------------------------------------
    // Re-audit — post-apply consistency check
    // ------------------------------------------------------------------
    let re_drift = match audit::run_audit(&config.corpus_path, &config.template) {
        Ok(d) => d,
        Err(e) => {
            return OrchestratorOutcome::Failed {
                steps,
                step_name: "re-audit".to_string(),
                error: e.to_string(),
            };
        }
    };
    let residual = re_drift.iter().filter(|e| !e.category.is_informational()).count();
    steps.push(StepOutcome::ReAudit { residual_drift: residual });

    let exit_code = if residual > 0 { 1 } else { 0 };
    OrchestratorOutcome::Complete { steps, exit_code }
}

// ---------------------------------------------------------------------------
// Summary rendering (used by the CLI subcommand)
// ---------------------------------------------------------------------------

pub fn print_pipeline_summary(outcome: &OrchestratorOutcome, out: &mut dyn std::io::Write) {
    for step in outcome.steps() {
        match step {
            StepOutcome::Audit { drift_count, blocking } => {
                let _ = writeln!(
                    out,
                    "  audit: {} blocking drift item(s){}",
                    drift_count,
                    if *blocking { " [blocking]" } else { "" }
                );
            }
            StepOutcome::Plan { main_ops, fm_ops, gap_rows } => {
                let _ = writeln!(
                    out,
                    "  plan: {} structural op(s), {} FM op(s), {} gap row(s)",
                    main_ops, fm_ops, gap_rows
                );
            }
            StepOutcome::ApplySkipped => {
                let _ = writeln!(out, "  apply: skipped (dry-run)");
            }
            StepOutcome::AnchorApply { .. } => {
                let _ = writeln!(out, "  anchor apply: OK");
            }
            StepOutcome::AnchorApplyFailed { diagnostic, .. } => {
                let _ = writeln!(out, "  anchor apply: FAILED — {diagnostic}");
            }
            StepOutcome::AnchorFmMigrate { .. } => {
                let _ = writeln!(out, "  anchor frontmatter migrate: OK");
            }
            StepOutcome::AnchorFmMigrateFailed { diagnostic, .. } => {
                let _ = writeln!(out, "  anchor frontmatter migrate: FAILED — {diagnostic}");
            }
            StepOutcome::GapReport { files_written, gap_dir } => {
                let _ = writeln!(
                    out,
                    "  gap-report: {} file(s) written to '{}'",
                    files_written,
                    gap_dir.display()
                );
            }
            StepOutcome::ReAudit { residual_drift } => {
                if *residual_drift > 0 {
                    let _ = writeln!(
                        out,
                        "  re-audit: {} residual blocking drift item(s) remain",
                        residual_drift
                    );
                } else {
                    let _ = writeln!(out, "  re-audit: clean — no residual drift");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::anchor_runner::MockAnchorRunner;
    use crate::template::TemplateLoader;
    use tempfile::TempDir;

    fn setup_workspace_with_clean_corpus() -> (TempDir, OrchestratorConfig) {
        let dir = TempDir::new().unwrap();
        let tmpl_dir = dir.path().join(".accelmars/canon/templates/test-orch");
        std::fs::create_dir_all(&tmpl_dir).unwrap();
        std::fs::write(
            tmpl_dir.join("manifest.toml"),
            r#"name = "test-orch"
version = "1.0.0"
description = "test"
[folder_rules]
shape = "numbered-tiers"
"#,
        )
        .unwrap();

        let corpus = dir.path().join("corpus");
        std::fs::create_dir_all(corpus.join("01-identity")).unwrap();
        std::fs::write(corpus.join("01-identity/_INDEX.md"), "").unwrap();

        let gap_dir = corpus.join("41-gaps");
        let loader = TemplateLoader::from_workspace_root(dir.path());
        let template = loader.load_by_name("test-orch").unwrap();

        let config = OrchestratorConfig {
            corpus_path: corpus,
            template,
            apply: false,
            gap_report_dir: gap_dir,
            workspace_root: dir.path().to_owned(),
        };
        (dir, config)
    }

    #[test]
    fn dry_run_clean_corpus_exits_0() {
        let (_dir, config) = setup_workspace_with_clean_corpus();
        let runner = MockAnchorRunner::succeeds();
        let outcome = run_pipeline(&config, &runner);
        assert_eq!(outcome.exit_code(), 0);
        let has_skipped = outcome.steps().iter().any(|s| matches!(s, StepOutcome::ApplySkipped));
        assert!(has_skipped, "dry-run should have ApplySkipped step");
    }

    #[test]
    fn dry_run_writes_no_files() {
        let (_dir, config) = setup_workspace_with_clean_corpus();
        let runner = MockAnchorRunner::succeeds();
        run_pipeline(&config, &runner);
        // gap_dir should not exist (nothing written in dry-run)
        assert!(!config.gap_report_dir.exists(), "no files should be written in dry-run");
    }

    #[test]
    fn idempotence_clean_corpus_apply_is_noop() {
        let (dir, mut config) = setup_workspace_with_clean_corpus();
        config.apply = true;
        let gap_dir = dir.path().join("corpus/41-gaps");
        config.gap_report_dir = gap_dir.clone();
        // 41-gaps must have _INDEX.md to be a conformant numbered folder.
        std::fs::create_dir_all(&gap_dir).unwrap();
        std::fs::write(gap_dir.join("_INDEX.md"), "").unwrap();

        let runner = MockAnchorRunner::succeeds();
        let outcome = run_pipeline(&config, &runner);
        assert_eq!(outcome.exit_code(), 0);

        // No gap files written — clean corpus has zero judgment cases.
        let gap_files: Vec<_> = std::fs::read_dir(&gap_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("CANON-"))
            .collect();
        assert!(gap_files.is_empty(), "clean corpus should produce zero gap files");
    }
}
