# Built-in Templates

Canon ships built-in templates that work out of the box without any installation.
Built-in templates are compiled into the binary — they are always available.

> **Rule:** Built-in templates contain no AccelMars-specific content. They are
> generic reference shapes. AccelMars's own template lives in workspace config
> at `.accelmars/canon/templates/accelmars-standard/`, not here.

---

## Template Listing

Use `canon template list` to see all available templates with their tier and description.

---

## `canon-default`

**Tier:** Built-in  
**Audience:** Anyone using canon's standard 8-phase refinery pipeline  
**When to use:** You ran `canon refine` and want to audit or realign the output. This
template encodes the fixed output shape that canon v0.1.0 emits.

### Output Shape

Canon's refinery pipeline produces the following top-level structure:

```
<output>/
├── atoms/
│   └── <domain>/
│       └── atoms.json          # per-domain atom collections
├── domains/
│   └── <domain>/               # domain charters and category files
├── load-packs/
│   └── <domain>/               # pre-assembled LLM context packs
├── archive/                    # original pre-normalization files
├── .canon/                     # intermediate state (manifest.json, cache/, consolidation)
└── gaps/                       # gap-report files (produced by canon align)
```

The shape is `custom` (not numbered-tiers or flat) because the top-level folders are
fixed by canon's pipeline, not user-configurable.

### Frontmatter Schema

The included `frontmatter.schema.json` covers generic fields used by documents in
canon's output:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | No | Atom ID in `domain::aspect::detail` format |
| `category` | string | No | Knowledge category (C1–C8 or custom string) |
| `status` | string | No | Lifecycle state: `active`, `stale`, `archived` |
| `domain` | string | No | Domain this document belongs to |
| `title` | string | No | Human-readable title |
| `version` | string | No | Document version (semver recommended) |
| `created` | string | No | Creation date (ISO 8601) |
| `updated` | string | No | Last-updated date (ISO 8601) |
| `tags` | array | No | Free-form tags |

`additionalProperties: true` — custom fields are allowed.

### Invariants

| Invariant | Value | Notes |
|-----------|-------|-------|
| `gaps_folder` | `gaps` | Where `canon align` writes residual gap reports |
| `atomic_file_gate` | `false` | Canon emits multi-atom `atoms.json` files |
| `index_required` | `false` | Not enforced in canon's default output |

---

## Adding More Templates

To add a workspace-specific template, create a directory at:

```
.accelmars/canon/templates/<your-template-name>/
├── manifest.toml
└── frontmatter.schema.json   # optional
```

See `docs/template-format.md` for the full manifest specification.

A workspace template with the same name as a built-in overrides the built-in silently.
`canon template list` shows the tier for each template so the override is visible.

---

_AccelMars Co., Ltd. — canon built-in templates_
