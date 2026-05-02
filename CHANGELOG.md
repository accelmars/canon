# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-05-02

### Added
- CANON-001 — `canon align --apply` emits `_INDEX.md` stubs for numbered folders missing an index file; stub contains correct frontmatter, TOC pre-filled from folder contents, TODO marker for operator summary.
- AENG-011 — `canon align` reads engine class from `01-identity/_INDEX.md` and auto-assigns engine-class folder IDs from `[[engine_class_extensions]]` in `folder-rules.toml`; eliminates `[IdAssignment]` gaps for known engine classes.
- AENG-012 — `accelmars-standard` template gains `inference-rules.toml` with stem/constant rules for `15-providers`, `16-mock`, `31-evals`; `frontmatter.schema.json` x-synonyms extended with `current → active`.

### Fixed
- Plan emitter emits `ops = []` for empty structural op lists — `anchor apply` no longer rejects zero-op plans with `missing field 'ops'`.

## [0.1.0] - 2026-05-01

### Features
- Root `--version` / `-V` and `--help` / `-h` flags; no-subcommand now prints help instead of error

### Bug Fixes
- Regenerate release workflow from dist 0.31.0 ([#12](https://github.com/accelmars/canon/pull/12))

## [0.0.2] - 2026-04-29

### Features
- Canon template namespace — list/show/validate/install subcommands with three-tier discovery, JSON show output, and local/git-URL install (#9) ([#9](https://github.com/accelmars/canon/pull/9))
- Canon align --apply — Audit→Plan→anchor apply→gap report→re-audit orchestrator; writes CANON-NNN gap files for judgment cases (#8) ([#8](https://github.com/accelmars/canon/pull/8))
- Canon align --output — emits anchor-compatible structural plan + frontmatter migration plan for corpus drift (#6) ([#6](https://github.com/accelmars/canon/pull/6))
- Canon audit subcommand — read-only drift report against any structure template (#5) ([#5](https://github.com/accelmars/canon/pull/5))
- Built-in canon-default template — canon's v0.1.0 output shape compiled into the binary; boundary-enforcing tests and reference docs (#4) ([#4](https://github.com/accelmars/canon/pull/4))
- Structure-template format and loader — declarative TOML manifests, three-tier resolver (workspace/user/built-in), and typed error model for canon-core (#2) ([#2](https://github.com/accelmars/canon/pull/2))


### Bug Fixes
- (**docs**) Canonical CHANGELOG format and CODEOWNERS


