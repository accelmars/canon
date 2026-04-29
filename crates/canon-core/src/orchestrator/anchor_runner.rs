use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AnchorMissingError {
    pub message: String,
}

impl std::fmt::Display for AnchorMissingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AnchorMissingError {}

#[derive(Debug, Clone)]
pub struct RunnerError {
    pub exit_code: i32,
    pub diagnostic: String,
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "anchor exited {} — {}", self.exit_code, self.diagnostic)
    }
}

impl std::error::Error for RunnerError {}

// ---------------------------------------------------------------------------
// AnchorRunner trait
// ---------------------------------------------------------------------------

pub trait AnchorRunner: Send + Sync {
    /// Check that `anchor frontmatter --help` exits 0 (AENG-006 capability gate).
    fn check_frontmatter_capability(&self) -> Result<(), AnchorMissingError>;
    /// Run `anchor apply <plan_path>` — execute structural moves atomically.
    fn run_apply(&self, plan_path: &Path) -> Result<(), RunnerError>;
    /// Run `anchor frontmatter migrate <plan_path>` — execute FM migrations.
    fn run_frontmatter_migrate(&self, plan_path: &Path) -> Result<(), RunnerError>;
}

// ---------------------------------------------------------------------------
// DefaultAnchorRunner — shells out to the real anchor binary
// ---------------------------------------------------------------------------

pub struct DefaultAnchorRunner;

impl AnchorRunner for DefaultAnchorRunner {
    fn check_frontmatter_capability(&self) -> Result<(), AnchorMissingError> {
        check_anchor_frontmatter()
    }

    fn run_apply(&self, plan_path: &Path) -> Result<(), RunnerError> {
        let output = Command::new("anchor")
            .args(["apply", plan_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| RunnerError {
                exit_code: -1,
                diagnostic: format!("failed to launch anchor: {e}"),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(RunnerError {
                exit_code: output.status.code().unwrap_or(-1),
                diagnostic: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        }
    }

    fn run_frontmatter_migrate(&self, plan_path: &Path) -> Result<(), RunnerError> {
        let output = Command::new("anchor")
            .args(["frontmatter", "migrate", plan_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| RunnerError {
                exit_code: -1,
                diagnostic: format!("failed to launch anchor: {e}"),
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(RunnerError {
                exit_code: output.status.code().unwrap_or(-1),
                diagnostic: String::from_utf8_lossy(&output.stderr).to_string(),
            })
        }
    }
}

/// Check whether `anchor frontmatter --help` exits 0 (AENG-006 capability gate).
///
/// Called before any `--apply` mode execution to give the operator a clear
/// diagnostic if anchor v0.6.0 is not yet installed.
pub fn check_anchor_frontmatter() -> Result<(), AnchorMissingError> {
    let result = Command::new("anchor")
        .args(["frontmatter", "--help"])
        .output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(AnchorMissingError {
            message: "anchor frontmatter not found; ensure accelmars/anchor v0.6.0+ is installed \
                      (delivered by AENG-006)"
                .to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// MockAnchorRunner — deterministic test double
// ---------------------------------------------------------------------------

pub struct MockAnchorRunner {
    pub capability_ok: bool,
    pub apply_ok: bool,
    pub apply_diagnostic: String,
    pub fm_migrate_ok: bool,
    pub fm_migrate_diagnostic: String,
}

impl MockAnchorRunner {
    pub fn succeeds() -> Self {
        Self {
            capability_ok: true,
            apply_ok: true,
            apply_diagnostic: String::new(),
            fm_migrate_ok: true,
            fm_migrate_diagnostic: String::new(),
        }
    }

    pub fn anchor_missing() -> Self {
        Self {
            capability_ok: false,
            apply_ok: false,
            apply_diagnostic: String::new(),
            fm_migrate_ok: false,
            fm_migrate_diagnostic: String::new(),
        }
    }

    pub fn apply_fails(diagnostic: &str) -> Self {
        Self {
            capability_ok: true,
            apply_ok: false,
            apply_diagnostic: diagnostic.to_string(),
            fm_migrate_ok: true,
            fm_migrate_diagnostic: String::new(),
        }
    }
}

impl AnchorRunner for MockAnchorRunner {
    fn check_frontmatter_capability(&self) -> Result<(), AnchorMissingError> {
        if self.capability_ok {
            Ok(())
        } else {
            Err(AnchorMissingError {
                message: "anchor frontmatter not found; ensure accelmars/anchor v0.6.0+ is \
                          installed (delivered by AENG-006)"
                    .to_string(),
            })
        }
    }

    fn run_apply(&self, _plan_path: &Path) -> Result<(), RunnerError> {
        if self.apply_ok {
            Ok(())
        } else {
            Err(RunnerError {
                exit_code: 1,
                diagnostic: self.apply_diagnostic.clone(),
            })
        }
    }

    fn run_frontmatter_migrate(&self, _plan_path: &Path) -> Result<(), RunnerError> {
        if self.fm_migrate_ok {
            Ok(())
        } else {
            Err(RunnerError {
                exit_code: 1,
                diagnostic: self.fm_migrate_diagnostic.clone(),
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_succeeds_returns_ok() {
        let runner = MockAnchorRunner::succeeds();
        assert!(runner.check_frontmatter_capability().is_ok());
        assert!(runner.run_apply(Path::new("/tmp/plan.toml")).is_ok());
        assert!(runner
            .run_frontmatter_migrate(Path::new("/tmp/fm.toml"))
            .is_ok());
    }

    #[test]
    fn mock_anchor_missing_returns_error() {
        let runner = MockAnchorRunner::anchor_missing();
        let err = runner.check_frontmatter_capability().unwrap_err();
        assert!(err.message.contains("AENG-006"), "msg={}", err.message);
    }

    #[test]
    fn mock_apply_fails_returns_diagnostic() {
        let runner = MockAnchorRunner::apply_fails("AENG-002: ref integrity violation at line 3");
        let err = runner.run_apply(Path::new("/tmp/plan.toml")).unwrap_err();
        assert_eq!(err.exit_code, 1);
        assert!(err.diagnostic.contains("AENG-002"));
    }
}
