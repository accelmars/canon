# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
