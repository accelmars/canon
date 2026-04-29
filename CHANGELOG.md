# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added — CFC-201 — `canon align --output` (mechanical anchor-plan emission with closed-layer reservation)

- `canon align <corpus-path> --template <name|path> --output <plan.toml> [--frontmatter-output <fm-plan.toml>]`: consumes a drift report (CFC-101) and emits an anchor-compatible structural TOML plan for folder renames/creates plus a frontmatter migration sibling plan.
- Exit codes: 0 = no drift, 1 = drift found (plan written), 2 = error.
- `canon-core::plan` module: `MechanicalPlanEmitter`, `JudgmentEmitter` trait (open/closed boundary), `DefaultJudgmentEmitter` (gap-report-only open stub), `MainPlan`/`MainPlanOp`, `FmPlan`/`FmPlanOp`, `GapReportRow`, `JudgmentCase`.
- `MainPlanOp`: `CreateDir { path }` and `Move { src, dst }` — anchor plan schema v1 compatible.
- `FmPlanOp`: `AddField`/`SetField` — consumed by `anchor frontmatter migrate` (AENG-006, sibling project).
- `JudgmentEmitter` trait: open/closed seam. Closed impl (`canon_judgment::StubJudgmentEmitter`) gap-reports all judgment cases in v1 per Rule 6 (no AI auto-resolution).
- FolderShape drift → `Move` op when folder-rules.toml has a reverse mapping; otherwise `IdAssignment` gap row.
- Frontmatter drift → FM plan ops (`AddField`/`SetField` with `FIXME` placeholder values for human review).
- Graduation/ContentSplit drift → `GraduationBoundary` gap rows.
- Plan paths emitted workspace-root-relative so `anchor plan validate` resolves src paths correctly.
- Atomic writes via write-to-temp-then-rename; deterministic TOML output (stable op ordering).
- `tests/align_roundtrip.rs`: 9 integration tests — clean baseline, drift baseline, round-trip op equivalence, `anchor plan validate` cross-check, boundary audit (no `canon_judgment` symbols in open binary), determinism.


### Added — CFC-101 — canon audit subcommand (template-driven read-only diff)

- `canon audit <corpus-path> --template <name|path> [--format table|json|markdown]`: walks a corpus directory, loads a structure template via the CFC-050 loader, and reports all drift against that template's folder rules, frontmatter schema, and invariants.
- Exit codes: 0 = conformant, 1 = blocking drift found, 2 = error.
- `canon-core::audit` module: `run_audit(corpus_root, template)` → `Vec<DriftEntry>`.
- `DriftCategory` enum: `FolderShape`, `MissingIndex`, `FrontmatterRequiredMissing`, `FrontmatterTypeWrong`, `FrontmatterValueInvalid`, `GraduationCandidate`, `ContentSplitSuggested`, `UnknownFieldInfo` (informational), `InvariantViolation`.
- `audit::folder`: numbered-tier and flat folder-shape checks + `_INDEX.md` presence.
- `audit::frontmatter`: YAML frontmatter parser + JSON-Schema-style validator (required fields, type checks, enum checks, allOf conditions).
- `audit::invariants`: graduation-candidate heuristic (>500 lines), content-split heuristic (≥4 H2 sections with `atomic_file_gate`), gaps-folder-as-file invariant.
- `canon-core::cli::audit`: `run_impl` follows Rule 13 (CWD-relative path resolution, injected writers for testability).
- `tests/audit_baselines.rs`: 16 integration tests — clean baseline, gateway-engine drift baseline, 9 per-DriftCategory synthetic fixtures, 2 error paths, 2 format-flag tests.


### Added — CFC-051 — built-in canon-default template (canon's existing canonical shape)

- `templates/canon-default/manifest.toml`: built-in template encoding canon v0.1.0's output shape — `custom` folder shape (atoms/, domains/, load-packs/, archive/, .canon/), `gaps` gap-report folder, `atomic_file_gate = false`.
- `templates/canon-default/frontmatter.schema.json`: JSON Schema 2020-12 covering generic atom-frontmatter fields (id, category, status, domain, title, version, created, updated, tags). No AccelMars-specific content.
- `docs/built-in-templates.md`: documents each built-in template, its output shape, frontmatter schema fields, and invariants.
- `canon-core::template::loader`: `production_builtins()` helper returns compiled built-in registry for test use.
- `COMPILED_BUILT_INS` in loader: `canon-default` registered via `include_str!()` — available in every canon binary without filesystem installation.
- `tests/built_in_templates.rs`: 5 integration tests — parse, format-spec conformance, list appearance, frontmatter schema reference, boundary (no accelmars strings in built-ins).

### Added — CFC-050 — structure-template format and loader (canon-core)

- `docs/template-format.md`: public spec for the structure-template TOML format — manifest fields, folder shapes, frontmatter schema reference, invariants, three-tier resolution semantics, error model.
- `canon-core::template`: new module exposing `TemplateManifest`, `FolderRules`, `FolderShape`, `Invariants`, `FrontmatterRef`, `NamingConventions`, `TemplateLoader`, `LoadedTemplate`, `ListedTemplate`, `TemplateTier`, `TemplateError`.
- `TemplateLoader::from_workspace_root`: production constructor; resolves workspace (`.accelmars/canon/templates/`) and user (`~/.config/canon/templates/`) tiers from workspace root path.
- `TemplateLoader::with_builtins`: test-friendly constructor accepting explicit paths and explicit built-in registry (Rule 13 `_impl` pattern).
- Three-tier name resolution: workspace > user > built-in (workspace wins on name collision).
- `load_by_name`, `load_by_path`, `list_all` functions.
- Typed `TemplateError` with `NotFound` (names all searched paths), `Malformed`, `MissingSchema`, `Io` variants.
- `template::validate`: post-load validation (schema file existence, invariant coherence).
- Integration tests: 7 cases covering all resolution tiers, explicit-path override, missing-template error, malformed-manifest rejection, and tier-precedence.

- Initial scaffold: Cargo workspace with `canon-core` placeholder, CI, and public-repo standard files.
