# Tasks: APRepo APK Download and Processing Manager

**Input**: Design documents from `/specs/001-aprepo-apk-manager/`
**Prerequisites**: plan.md, spec.md, data-model.md, contracts/, research.md, quickstart.md

**Organization**: Tasks grouped by user story to enable independent implementation and testing.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Initialize Rust project, set up dependencies, create directory structure.

- [X] T001 Create project structure per implementation plan in `Cargo.toml`, `src/`, `tests/integrational/`

**Checkpoint**: `cargo build` succeeds with a stub `main.rs`.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T002 [P] Implement Config module in `src/config.rs` — YAML parsing, `$VARIABLE` interpolation, validation (uniqueness, PlayStore policy, directory existence)
- [X] T003 [P] Implement State module in `src/state.rs` — `state.yaml` read/write, version records keyed by `source:identifier`, `SourceCapability` tracking per source
- [X] T004 [P] Implement Lock module in `src/lock.rs` — exclusive `flock` on `state.yaml` at startup, RAII release
- [X] T005 [P] Implement Logging utility in `src/util/logging.rs` — stdout for progress, stderr for errors/warnings/debug with `--verbose` gating
- [X] T006 [P] Implement ZIP validation in `src/util/zip_validate.rs` — well-formed ZIP check using the `zip` crate (CRC validation)

**Checkpoint**: Foundation ready — `cargo test` passes; config loads, validates, interpolates env vars; state persists; lock works.

---

## Phase 3: User Story 1 — Download Packages with Throttling and Version Awareness (Priority: P1) 🎯 MVP

**Goal**: The repository maintainer runs a command to check for and download updates for all configured packages. The system checks each package's current version against the cached version, respects throttling rules, and only downloads when a newer version is available or when a force-refresh is requested.

**Why this priority**: Without reliable downloads, the repository contains stale or missing packages. This is the core value proposition of the tool.

**Independent Test**: Can be fully tested by configuring a few packages from different sources, running the download command, and verifying that only new or changed versions are fetched while throttle delays are honored.

### Implementation for User Story 1

- [X] T007 Implement Download Orchestrator in `src/download/mod.rs` — sequential scheduler that iterates packages, checks throttle/version, invokes backend trait, validates ZIP, retries once, updates state, honors `--force` and `--package` flags
- [X] T008 [P] [US1] Implement apkeep Backend in `src/download/apkeep.rs` — wrapper for PlayStore/RuStore/APKPure; detects per-architecture capability, falls back to universal download; supports `--list-versions` for metadata mode
- [X] T009 [P] [US1] Implement GitHub Backend in `src/download/github.rs` — `gh` CLI + API wrapper; queries latest release tag, matches assets by `mask`/`arch_masks`, downloads selected asset
- [X] T010 [P] [US1] Implement WebDL Backend in `src/download/webdl.rs` — HTTP direct download with `reqwest`; HEAD request for metadata version hash; supports `arch_files` per-architecture downloads

**Checkpoint**: `aprepo download --config test.yaml` downloads fixtures sequentially with throttling.

---

## Phase 4: User Story 2 — Process Cached Files into F-Droid-Compatible APKs (Priority: P2)

**Goal**: The maintainer runs a processing command that transforms everything in the cache directory into clean, standalone APK files in the output directory. XAPK bundles are optionally merged into single APKs based on target device preferences.

**Independent Test**: Place a mix of APK and XAPK files in the cache directory, run the process command, and verify that the output directory contains valid artifacts: standalone APK files and, when `settings.repack_xapk` is `false`, original XAPK files.

### Implementation for User Story 2

- [X] T011 Implement Process Orchestrator in `src/process/mod.rs` — scan cache directory, compare `mtime` against output, process new/changed files, honor `--force` and `--package` flags
- [X] T012 [P] [US2] Implement APK Copy + Manifest Extraction in `src/process/apk.rs` — validate ZIP, extract `package` and `versionName` from `AndroidManifest.xml`, copy to output with `package_name_version_architecture.apk` naming
- [X] T013 [P] [US2] Implement XAPK Extraction and Manifest Parsing in `src/process/xapk.rs` — read XAPK as ZIP, parse `manifest.json`, identify base APK and splits (architecture, density)
- [X] T014 [US2] Implement XAPK-to-APK Merging in `src/process/xapk.rs` — full 12-step apktool decode/merge/rebuild cycle per FR-009c: extract all APKs → classify by type → decode with `apktool d -s` → per-arch copy of decoded base → merge `lib/` from arch splits → merge `res/` from DPI/locale splits (skip `values/public.xml`, skip existing `drawable*`) → merge `assets/assetpack/` → merge `doNotCompress` from `apktool.yml` → delete `BNDLTOOL.*` → fix misnamed `.png` → update `AndroidManifest.xml` (remove `isSplitRequired`, change stamp type) → rebuild with `apktool b` → `zipalign -p -f 4` → `apksigner sign`
- [X] T015 [US2] Write XAPK Merge Unit Tests in `src/process/xapk.rs` (`#[cfg(test)]`) — 16 tests covering: APK classification (main/arch/dpi/locale), DPI priority ordering, `doNotCompress` extract/replace/deduplicate, manifest surgery (all 6 replacements verified), misnamed image fix (JPEG-in-PNG detection), signature file deletion, resource merge (public.xml skip + drawable preservation), arch lib tree copy, assetpack copy, recursive directory copy
- [X] T016 [US2] Implement Signing Integration in `src/process/xapk.rs` — apply PKCS12 signing via `apksigner` to repacked APKs only; direct-download APKs copied unchanged
- [X] T017 [US2] Implement Retention & Cleanup in `src/process/mod.rs` — purge old versions exceeding `retention_depth` for all packages; remove artifacts for packages no longer in configuration

**Checkpoint**: `aprepo process --config test.yaml` produces valid output APKs from cache.

---

## Phase 5: User Story 3 — Manage Repository via Simple CLI Commands (Priority: P3)

**Goal**: The maintainer interacts with the entire system through a small set of CLI commands with a configuration file. Commands are intuitive, accept a configuration path, and provide clear feedback.

**Independent Test**: Invoke each CLI command with valid and invalid arguments and verify appropriate behavior, help text, and exit codes.

### Implementation for User Story 3

- [X] T018 [P] [US3] Define clap CLI in `src/main.rs` — subcommands (`bootstrap`, `download`, `process`), flags (`--config`, `--verbose`, `--force`, `--package`), help text matching CLI contract
- [X] T019 [US3] Implement Bootstrap Command in `src/main.rs` — generate template YAML config with `settings` and `sources` keys, create PKCS12 keystore via `keytool` with constants matching template (`BOOTSTRAP_KEYSTORE_PASSWORD`, `BOOTSTRAP_KEY_ALIAS`, `BOOTSTRAP_KEY_PASSWORD`)
- [X] T020 [US3] Implement Default Run in `src/main.rs` — when no subcommand given, run `download`, then `process`, then cleanup in sequence; total failure stops before process; partial failure continues
- [X] T021 [US3] Implement Package Filter in `src/main.rs` — `--package` / `-p` limits download to matching store-source package and process to cached APKs with matching manifest package

**Checkpoint**: All commands and flags work; default run chains download+process.

---

## Phase 6: Integration Tests

**Purpose**: End-to-end validation with stubbed backends and synthetic fixtures.

- [X] T022 [P] Create Makefile + Test Directory Structure in `tests/integrational/Makefile`, `tests/integrational/invalid/`, `tests/integrational/sources/`, `tests/integrational/xapk/`, `tests/integrational/fixtures/`
- [X] T023 [P] Create Synthetic APK Fixture Generator in `tests/integrational/fixtures/generate.sh` — produce `.apk` files that pass `aapt2 dump badging`; produce XAPK bundles with base + arch + density splits
- [X] T024 [P] Implement Invalid Config Tests in `tests/integrational/invalid/` — invalid source name, duplicate package names, PlayStore with `metadata` policy, missing `cache_dir`/`output_dir`, missing `$VARIABLE` env var
- [X] T025 [P] Implement Per-Source Valid Config Tests in `tests/integrational/sources/` — one fixture per source (`google_play/`, `rustore/`, `apkpure/`, `github/`, `webdl/`); test per-architecture downloads; verify well-formed ZIP in cache and valid APK in output
- [X] T026 [P] Implement Metadata Policy Message Tests in `tests/integrational/sources/` — verify cache-hit and new-version informational messages for GitHub, WebDL, and apkeep-backed sources
- [X] T027 [P] Implement Token Syntax Tests in `tests/integrational/` — direct token and `$VARIABLE` syntax; unset env var → non-zero exit
- [X] T028 [P] Implement XAPK Repack Tests in `tests/integrational/xapk/` — `repack_xapk: true` produces `aapt2`-valid APK(s); `repack_xapk: false` copies XAPK unchanged; signing fixture verifies signed output

**Checkpoint**: `make` in `tests/integrational/` passes all fixtures.

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect all user stories.

- [X] T029 Review all error paths in `src/` for clear messages and correct exit codes — verify "ERROR: " and "WARNING: " prefixes; Ctrl+C leaves cache consistent; repeated warnings deduplicated (FR-012b)
- [X] T030 End-to-End Validation — run full cycle against synthetic fixtures; `cargo test` passes all unit tests; `make` passes all integration tests; `cargo clippy` reports zero warnings
- [ ] T031 Documentation Review — verify `quickstart.md` produces working setup from fresh clone; `contracts/cli-interface.md` matches `--help` output; `contracts/config-schema.md` matches validation behavior

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately.
- **Foundational (Phase 2)**: Depends on Setup (T001). All Phase 2 tasks are parallel once T001 is done.
- **User Story 1 (Phase 3)**: Depends on Foundational (T002–T006). T008–T010 are parallel once T007 is done.
- **User Story 2 (Phase 4)**: Depends on Foundational (T002–T006). T012–T013 are parallel once T011 is done.
- **User Story 3 (Phase 5)**: Depends on Foundational (T002–T006) and US1 (T007) + US2 (T011).
- **Integration Tests (Phase 6)**: Depends on all implementation phases.
- **Polish (Phase 7)**: Depends on all implementation + integration tests.

### Task Dependency Graph

```
T001 → T002, T003, T004, T005, T006
T002–T006 → T007
T007 → T008, T009, T010
T002–T006 → T011
T011 → T012, T013
T013 → T014
T014 → T015
T014, T015 → T016
T012–T016 → T017
T002, T007, T011 → T019, T020, T021
T018 → T019, T020, T021
All above → T022–T028
All above → T029 → T030 → T031
```

### Parallel Opportunities

- **Phase 2 (Foundational)**: T002, T003, T004, T005, T006 all in parallel.
- **Phase 3 (US1)**: T008, T009, T010 in parallel after T007.
- **Phase 4 (US2)**: T012, T013 in parallel after T011.
- **Phase 6 (Tests)**: T022–T028 mostly in parallel.

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001)
2. Complete Phase 2: Foundational (T002–T006)
3. Complete Phase 3: User Story 1 (T007–T010)
4. **STOP and VALIDATE**: `cargo test` passes; `aprepo download --config test.yaml` works against stubs.

### Incremental Delivery

1. Setup + Foundational → Foundation ready
2. Add User Story 1 → Download works → Demo (MVP!)
3. Add User Story 2 → Process works → Demo
4. Add User Story 3 → Full CLI → Demo
5. Add Integration Tests → `make` passes
6. Polish → `cargo clippy` clean → Ship

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together.
2. Once Foundational is done:
   - Developer A: User Story 1 (T007–T010)
   - Developer B: User Story 2 (T011–T017)
   - Developer C: User Story 3 (T018–T021) + Integration Tests (T022–T028)
3. Stories complete and integrate independently.

---

## Milestones

| Milestone | Tasks | Definition of Done |
|-----------|-------|---------------------|
| **M1 — Config & State** | T001–T006 | `cargo test` passes; config loads, validates, interpolates env vars; state persists; lock works |
| **M2 — Download** | T007–T010 | `aprepo download --config test.yaml` downloads fixtures sequentially with throttling |
| **M3 — Process** | T011–T017 | `aprepo process --config test.yaml` produces valid output APKs from cache |
| **M4 — Full CLI** | T018–T021 | All commands and flags work; default run chains download+process |
| **M5 — Integration Tests** | T022–T028 | `make` in `tests/integrational/` passes all fixtures |
| **M6 — Ship** | T029–T031 | `cargo clippy` clean; docs verified; ready for merge |

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests fail before implementing
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: vague tasks, same file conflicts, cross-story dependencies that break independence
