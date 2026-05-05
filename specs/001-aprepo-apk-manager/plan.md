# Implementation Plan: APRepo APK Download and Processing Manager

**Branch**: `001-aprepo-apk-manager` | **Date**: 2026-05-04 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-aprepo-apk-manager/spec.md`

## Summary

Build a standalone Rust CLI (`aprepo`) that downloads APK/XAPK from multiple sources (PlayStore, RuStore, APKPure, GitHub, direct URLs), caches them with version-aware throttling, optionally repacks XAPK bundles into per-architecture APKs, produces F-Droid-compatible output, and applies retention cleanup to manage disk space. The tool is configured via a single YAML file, uses external CLI backends (`apkeep`, `gh`), and must pass integration tests driven by a Makefile.

## Technical Context

**Language/Version**: Rust 1.78+ (edition 2021)
**Primary Dependencies**: `clap` (CLI), `serde`+`serde_yaml` (config), `zip` (archive validation), `quick-xml` (manifest extraction, fallback for plain XML), `fs2` (file locking), `reqwest` (WebDL/HEAD requests), `tokio` (async runtime for GitHub API calls), `tempfile` (temp dirs)
**External Tools (required in `$PATH`)**:
- Download: `apkeep` (PlayStore/RuStore/APKPure), `gh` (GitHub CLI)
- Process: `apktool` (decode/rebuild APKs), `zipalign` (page alignment), `apksigner` (PKCS12 signing), `aapt2` (binary AXML manifest extraction fallback)
**Storage**: Filesystem only — cache directory (raw downloads + `state.yaml`), output directory (processed APKs)
**Testing**: `cargo test` for unit tests; `tests/integrational/` with Makefile for integration tests using stubbed backends and local APK fixtures
**Target Platform**: Linux x86_64
**Project Type**: CLI application
**Constraints**: Sequential downloads only (no parallelism); per-source request delay; no hardcoded paths; all external tools resolved via `$PATH`; per-architecture backend capability detection with fallback
**Scale/Scope**: Single-user maintainer tool; repository sizes in tens of packages, not thousands

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Status |
|-----------|-------|--------|
| I. Think Before Coding | Spec is clarified with Q&As and explicit success criteria | Pass |
| II. Simplicity First | Single CLI with 3 subcommands; no web server, no DB, no daemon | Pass |
| III. Surgical Changes | Greenfield project — no existing code to preserve | Pass |
| IV. Goal-Driven Execution | Success criteria (SC-001 through SC-006) are measurable and testable | Pass |
| V. Regress-Resistant Fixes | Integration tests mandated; unit tests for XAPK merging required (FR-009b). Real-world testing produced 4 regression tests: arch cache path with dots, XAPK split ABI null fallback, explicit ABI precedence, aapt2 manifest attribute extraction | Pass |

**Gate Result**: All principles satisfied. Proceed to Phase 0.

### Post-Design Constitution Check (After Phase 1)

| Principle | Check | Status |
|-----------|-------|--------|
| I. Think Before Coding | All technical unknowns resolved in `research.md`; no assumptions left unspoken | Pass |
| II. Simplicity First | 8 direct dependencies; no abstraction layers beyond spec requirements; `std::process::Command` used instead of wrapper crates | Pass |
| III. Surgical Changes | Greenfield project — design is additive only | Pass |
| IV. Goal-Driven Execution | Every artifact (data-model, contracts, quickstart) traceable to a spec requirement or SC | Pass |
| V. Regress-Resistant Fixes | `tests/integrational/` with Makefile specified; XAPK reimplementation has explicit unit-test mandate (FR-009b). Real-world token testing discovered and fixed 4 bugs with regression tests: arch cache path with dots (file_stem truncation), APKPure ABI=null with underscore arch names, apkeep per-arch file naming, binary AXML requiring aapt2 fallback | Pass |

**Post-Design Gate Result**: All principles satisfied. Ready for task decomposition (`/speckit.tasks`).

## Project Structure

### Documentation (this feature)

```text
specs/001-aprepo-apk-manager/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
│   ├── cli-interface.md
│   └── config-schema.md
└── tasks.md             # Phase 2 output (future /speckit.tasks)
```

### Source Code (repository root)

```text
/home/necromant/Projects/fdroid-repo/v2/
├── Cargo.toml           # Workspace root
├── src/
│   ├── main.rs          # CLI entrypoint (clap subcommands)
│   ├── config.rs        # YAML config parsing, validation, env interpolation (FR-018b: arch validation)
│   ├── state.rs         # state.yaml read/write, version records, capabilities
│   ├── lock.rs          # Exclusive file lock on state.yaml
│   ├── download/
│   │   ├── mod.rs       # Download orchestrator (sequential scheduler)
│   │   ├── backend.rs   # Backend trait abstraction
│   │   ├── apkeep.rs    # apkeep wrapper (PlayStore, RuStore, APKPure)
│   │   ├── github.rs    # gh CLI + API wrapper
│   │   └── webdl.rs     # HTTP direct download + HEAD for metadata
│   ├── process/
│   │   ├── mod.rs       # Process orchestrator (cache -> output)
│   │   ├── xapk.rs      # XAPK extraction, 12-step merge (FR-009c: apktool decode/merge/rebuild, zipalign, sign), arch validation (FR-009d)
│   │   └── apk.rs       # APK validation, manifest extraction (quick-xml + aapt2 fallback for binary AXML)
│   └── util/
│       ├── zip_validate.rs  # ZIP well-formedness check
│       └── logging.rs       # stdout/stderr output helpers
├── tests/
│   └── integrational/
│       ├── Makefile         # Orchestrates all integration tests
│       ├── invalid/         # Invalid config fixtures
│       ├── sources/         # Per-source valid config fixtures
│       ├── xapk/            # XAPK repack on/off fixtures
│       └── fixtures/
│           └── *.apk          # Synthetic APK fixtures
└── Cargo.lock
```

**Structure Decision**: Single Rust binary crate (no workspace members). The `download/` and `process/` modules isolate the two major pipelines. `tests/integrational/` is a standalone directory with a Makefile that invokes the compiled `aprepo` binary against fixture configs.

## Complexity Tracking

> The 12-step XAPK-to-APK merge (FR-009c) is complex, but justified: a naive base-APK copy fails at install time with `INSTALL_FAILED_MISSING_SPLIT` because Android still sees split-required manifest metadata. The full apktool decode/merge/rebuild cycle is the only known working approach. Simpler alternatives (direct ZIP merge, skipping apktool) were rejected during real-world testing.

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| N/A | N/A | N/A |
