# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
# Changelog

All notable changes to this project will be documented in this file.



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
