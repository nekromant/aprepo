# Research Notes: APRepo APK Download and Processing Manager

## Decisions

### YAML Config Parsing

- **Decision**: `serde` + `serde_yaml`
- **Rationale**: Industry standard in Rust; derives structs from YAML; supports custom deserializers for `$VARIABLE` interpolation and duration strings (`24h`, `2s`). The `serde_yaml` crate is mature and widely used.
- **Alternatives considered**: `yaml-rust` (lower-level, no derive), `quick-yaml` (experimental). Rejected because `serde_yaml` offers the cleanest mapping to the spec's typed configuration schema.

### CLI Framework

- **Decision**: `clap` v4 with derive macros
- **Rationale**: De-facto standard for Rust CLI tools. Supports subcommands (`bootstrap`, `download`, `process`), global flags (`--config`, `--verbose`), and per-command flags (`--force`, `--package`) exactly as specified.
- **Alternatives considered**: `structopt` (deprecated, merged into clap v3+), manual `std::env::args`. Rejected — clap derive is the canonical path.

### ZIP Archive Validation

- **Decision**: `zip` crate
- **Rationale**: Pure-Rust ZIP reader that provides `Archive::by_index()` and CRC validation. Sufficient for the spec's "well-formed ZIP" requirement (`testzip()` equivalent). We will iterate all entries and verify CRCs.
- **Alternatives considered**: Shelling out to `unzip -t` (adds external dependency, rejected per spec), `async_zip` (unnecessary — downloads are sequential and file I/O is synchronous).

### AndroidManifest.xml Extraction

- **Decision**: `zip` crate to read `AndroidManifest.xml` from APK, then `quick-xml` for lightweight XML parsing
- **Rationale**: APKs are ZIPs. The manifest is at a fixed path inside the ZIP. `quick-xml` is fast, pull-parser style, and sufficient to extract `package` and `android:versionName` attributes without building a full DOM.
- **Alternatives considered**: `xml-rs` (slower, push-parser), `roxmltree` (DOM-based, heavier). Rejected — we only need two string attributes from a single tag.

### File Locking

- **Decision**: `fs2` crate (`fs2::FileExt::lock_exclusive`)
- **Rationale**: Thin, safe wrapper around POSIX `flock` (Linux) and `LockFileEx` (Windows). Exactly matches the spec requirement: "exclusive file lock on `state.yaml`".
- **Alternatives considered**: `fd-lock` (similar, but `fs2` is more widely used and simpler), `file-lock` (async-oriented, unnecessary). Rejected — `fs2` is the simplest cross-platform choice.

### HTTP Client for WebDL + GitHub API

- **Decision**: `reqwest` with `rustls-tls` feature
- **Rationale**: Standard Rust HTTP client. Supports sync and async modes, HEAD requests (for WebDL ETag/Last-Modified/Content-Length), and streaming downloads. `rustls` avoids OpenSSL system dependency issues.
- **Alternatives considered**: `ureq` (lighter, sync-only). Rejected — we need async for potential future extensions, and `reqwest` integrates cleanly with `tokio`.

### Async Runtime

- **Decision**: `tokio` with `rt-multi-thread` feature (lightweight usage)
- **Rationale**: `reqwest` requires a runtime. We only use async for GitHub API calls and WebDL HEAD requests; downloads themselves are orchestrated sequentially, so the runtime is not heavily loaded.
- **Alternatives considered**: `async-std`. Rejected — `tokio` is the ecosystem default and `reqwest` prefers it.

### XAPK-to-APK Merging

- **Decision**: Reimplemented from scratch in Rust; invoke `apktool`, `zipalign`, `apksigner` via `std::process::Command`
- **Rationale**: Spec explicitly rejects reusing the reference implementation (FR-009b). The orchestration (unpack base + splits, merge manifests/resources/DEX, repack, align, optionally sign) is reimplemented. External tools are invoked as subprocesses.
- **Alternatives considered**: Porting reference Python code. Rejected per spec mandate.

### Process Execution of External Tools

- **Decision**: `std::process::Command`
- **Rationale**: Built-in, zero-dependency. We need to run `apkeep`, `gh`, `apktool`, `zipalign`, `apksigner` with specific arguments and capture stdout/stderr.
- **Alternatives considered**: `duct` (convenience wrapper). Rejected — `std::process::Command` is sufficient for our needs; adding a dependency for minor ergonomics violates Simplicity First.

### XAPK Internal Structure

- **Decision**: Treat XAPK as a ZIP archive containing:
  - `manifest.json` (describing splits and package name)
  - One or more `.apk` files (base + architecture splits + density splits)
- **Rationale**: XAPK is not a standardized format, but the de-facto structure (used by APKPure and other stores) is a ZIP with a JSON manifest and embedded APKs. We parse the manifest to identify base vs. split APKs, then use the `zip` crate to extract them.
- **Alternatives considered**: Treating XAPK as a simple ZIP of APKs without parsing manifest. Rejected — the manifest is needed to distinguish base APK from splits and to know which split serves which architecture/density.

### Synthetic APK Fixture Generation

- **Decision**: Use `aapt2` (if available on host) or a small shell script that packages a minimal AndroidManifest.xml + empty resources into a ZIP with `.apk` extension.
- **Rationale**: Integration tests need real-enough APKs that `aapt2 dump badging` accepts. The fixture generation script lives in `tests/integrational/fixtures/generate.sh` and is invoked by the Makefile before tests run.
- **Alternatives considered**: Committing real APKs (binary bloat), using Python `androguard` (adds dependency). Rejected — `aapt2` is already a dependency for validation, so reusing it for fixture generation is consistent.

### Token / Credential Interpolation

- **Decision**: Custom pre-parse pass: after `serde_yaml` deserializes the config into a raw string map, recursively replace `$VAR_NAME` with `std::env::var(var_name)` before struct deserialization.
- **Rationale**: Keeps the YAML parser simple while supporting the spec's `$VARIABLE` syntax everywhere (tokens, keystore passwords, URLs). Errors if the variable is unset.
- **Alternatives considered**: Custom `serde` deserializer per field. Rejected — too verbose; a recursive string substitution pass is simpler and covers all fields uniformly.

### Duration Parsing

- **Decision**: Custom parser for `<number><unit>` strings (`h`, `d`, `s`, `m`) → `std::time::Duration`
- **Rationale**: Spec uses `24h`, `2s`, `7d` for `throttle_interval` and `delay_between_requests`. A small regex-based parser (or simple string split) is sufficient.
- **Alternatives considered**: `humantime` crate. Rejected — adds a dependency for a trivial parsing task; a 10-line function is simpler.

### Throttle Timestamp Storage

- **Decision**: Use file `mtime` of cached APK/XAPK as the throttle timestamp (per spec clarification)
- **Rationale**: No separate timestamp storage needed. `std::fs::metadata()?.modified()` provides the `SystemTime` to compare against `Duration`.
- **Alternatives considered**: Storing timestamps in `state.yaml`. Rejected — the spec explicitly says throttle timestamps are NOT in `state.yaml`.
