# Canon Structure-Template Format Specification

> **Version:** 0.1  
> **Status:** Stable  
> **Applies to:** canon v0.1.0+

This document defines the format for canon **structure templates** — declarative TOML files that describe a target output shape for canon's canonicalization pipeline. Templates are data, not code: contributing a new template means editing TOML files, not writing Rust.

---

## Overview

A structure template is a directory with at minimum a `manifest.toml` file and optionally a frontmatter schema:

```
<template-name>/
├── manifest.toml              # Required: template manifest (this spec)
└── frontmatter.schema.json    # Optional: JSON Schema 2020-12 for frontmatter validation
```

Template names must be lowercase kebab-case (e.g., `accelmars-standard`, `flat-with-tags`).

---

## Manifest Format

```toml
# Required: display name for this template
name = "example-template"

# Required: semantic version
version = "0.1.0"

# Required: one-line description shown in `canon template list`
description = "A short description of this template's purpose"

[folder_rules]
# Required: declares the folder shape this template targets.
# Supported shapes: "numbered-tiers", "flat", "by-domain", "custom"
shape = "numbered-tiers"

[frontmatter]
# Optional: path to a JSON Schema 2020-12 file, relative to this template directory.
# Absolute paths and URLs are rejected (security: no remote schema fetching).
schema = "frontmatter.schema.json"

[invariants]
# Optional: cross-file invariants canon enforces when auditing or aligning.

# If true, every numbered folder must contain an _INDEX.md file.
index_required = true

# If set, canon writes residual gap-report files into this folder name
# (relative to the output root). Must be non-empty if specified.
gaps_folder = "41-gaps"

# If true, each file must cover exactly one concept (atomic file gate).
atomic_file_gate = true

[naming_conventions]
# Optional: reserved for future naming rules. Currently unused.
```

---

## Folder Shapes

### `numbered-tiers`

Output is organized into numbered top-level folders (e.g., `10-intake/`, `20-foundations/`, `30-design/`). The tier numbering convention is defined by canon's default layout.

```toml
[folder_rules]
shape = "numbered-tiers"
```

### `flat`

All files live in a single directory with no subfolder hierarchy.

```toml
[folder_rules]
shape = "flat"
```

### `by-domain`

Files are grouped into domain-named subdirectories (e.g., `auth/`, `billing/`, `infra/`). Domain names are not prescribed by this spec; they emerge from the corpus.

```toml
[folder_rules]
shape = "by-domain"
```

### `custom`

Escape hatch for shapes that don't fit the above. No semantic enforcement — canon treats the corpus as structurally opaque and reports all drift as gap-report entries.

```toml
[folder_rules]
shape = "custom"
```

---

## Frontmatter Schema Reference

When `[frontmatter] schema` is set, canon loads the referenced JSON Schema file and uses it to:
- Validate frontmatter in `canon audit` (reports violations as drift)
- Infer missing required fields in `canon align`

**Constraints:**
- Path must be relative to the template directory
- Absolute paths are rejected
- URLs are rejected (no remote fetching; security boundary)
- If the file does not exist, `canon template validate` reports `MissingSchema`

```toml
[frontmatter]
schema = "frontmatter.schema.json"
```

---

## Invariants

Invariants are cross-file constraints evaluated after per-file processing.

| Field | Type | Default | Meaning |
|-------|------|---------|---------|
| `index_required` | bool | `false` | Every folder in the output must contain `_INDEX.md` |
| `gaps_folder` | string | (none) | Folder name where canon writes gap-report files |
| `atomic_file_gate` | bool | `false` | Each file must cover one concept |

When `align --apply` runs, violations that cannot be resolved mechanically become rows in the gap report (written to `<output>/<gaps_folder>/`).

---

## Three-Tier Resolution

Canon resolves templates by name using a three-tier search, in priority order (first match wins):

| Priority | Tier | Location |
|----------|------|----------|
| 1 (highest) | Workspace | `.accelmars/canon/templates/<name>/` relative to workspace root |
| 2 | User | `~/.config/canon/templates/<name>/` |
| 3 (lowest) | Built-in | Compiled into the canon binary |

A workspace template with the same name as a built-in **overrides** the built-in. `canon template list` shows the tier for each template so the override is visible.

### Explicit path override

`--template <path>` always wins; no name resolution is attempted.

```bash
canon audit . --template /path/to/my-template/
```

### Error on not found

When a template is not found by name, canon reports an error listing every path searched:

```
error: template 'my-template' not found; searched:
  /workspace/.accelmars/canon/templates/my-template
  /home/user/.config/canon/templates/my-template
```

---

## Error Model

| Error | Meaning |
|-------|---------|
| `NotFound` | Template name not found in any tier; message names all searched paths |
| `Malformed` | `manifest.toml` could not be parsed; message includes source path and TOML error |
| `MissingSchema` | `frontmatter.schema` references a file that does not exist |
| `Io` | Filesystem read error |

---

## Naming Conventions

The `[naming_conventions]` table is reserved for future file-naming rules (e.g., `kebab-case-only = true`). It is parsed and stored but not yet enforced in v0.1.

---

## Sample Manifests

### Numbered-tiers with frontmatter validation

```toml
name = "accelmars-standard"
version = "1.0.0"
description = "AccelMars workspace structure — numbered tiers, strict frontmatter"

[folder_rules]
shape = "numbered-tiers"

[frontmatter]
schema = "frontmatter.schema.json"

[invariants]
index_required = true
gaps_folder = "41-gaps"
atomic_file_gate = true
```

### Flat (minimal)

```toml
name = "flat-docs"
version = "0.1.0"
description = "Flat documentation structure — no subfolders, relaxed invariants"

[folder_rules]
shape = "flat"
```

---

## Design Decisions

**Why TOML?** Matches anchor's plan templates (`anchor plan list`). Readable without quoting hell. Consistent toolchain across the AccelMars stack.

**Why not JSON Schema for the manifest itself?** TOML is the manifest format. JSON Schema 2020-12 is only used for the *frontmatter schema reference* — a separate file that validates output file frontmatter, not the template manifest itself. Don't conflate.

**Why no remote schema fetching?** Security boundary. Schema resolution is always local and path-relative. Community templates that need remote schemas must bundle the schema file.

**Why is `custom` shape a no-op today?** It's an escape hatch for use cases canon doesn't yet anticipate. Making it a no-op rather than an error means early adopters can define templates for novel shapes without being blocked. The `gaps_folder` invariant still applies.

**Tier-precedence (workspace wins):** Workspace templates override built-ins because workspace config is the intended customization surface. AccelMars's `accelmars-standard` template lives in workspace config, not in canon's source tree — this is intentional boundary discipline.

---

_AccelMars Co., Ltd. — canon structure-template format spec v0.1_
