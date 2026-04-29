# canon

Open-layer canonicalization tooling for AccelMars foundations.

canon reads foundation corpus and structural specs, emits an anchor plan, and hands off to [anchor](https://github.com/accelmars/anchor) for execution. It is the proactive canonicalization layer of the AccelMars knowledge platform.

## Status

**In development.** See [issues](https://github.com/accelmars/canon/issues) for the canonicalization roadmap. This is the initial scaffold — commands and APIs are not yet available.

## Install

```bash
# Not yet published. Build from source once the first release lands:
cargo install --git https://github.com/accelmars/canon
```

Requires Rust 1.70+.

## Quick start

Coming soon. Track progress in the [canon-foundation-canonicalize project goal](https://github.com/accelmars).

## Architecture

canon operates in three phases:

1. **Audit** — reads a foundation directory and detects structural conformance gaps
2. **Plan** — converts gaps into an anchor execution plan
3. **Canonicalize** — orchestrates audit → plan → anchor handoff

The orchestrator (`canon foundation-canonicalize`) is open-layer. Judgment cases (graduation, type-ambiguous files, ID assignment) produce gap-report rows, not auto-fixes.

## When NOT to use canon

- **Replacing anchor directly.** canon produces plans; anchor executes them. Use anchor for file moves, reference validation, and workspace operations.
- **Closed-layer AI auto-resolution.** canon v1 does not attempt AI auto-resolution of judgment cases.
- **Non-AccelMars workspaces.** canon targets the `foundations/` corpus shape defined by AccelMars STRUCTURE.md.

## Telemetry

canon collects no telemetry. No data leaves your machine.

## License

Apache 2.0 — see [LICENSE](LICENSE).
