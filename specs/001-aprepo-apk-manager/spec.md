# Feature Specification: APRepo APK Download and Processing Manager

**Feature Branch**: `001-aprepo-apk-manager`
**Created**: 2026-05-03
**Status**: Draft
**Input**: User description: "Standalone CLI app (aprepo) that downloads APK/XAPK from multiple sources (via apkeep), caches them, optionally merges split APKs, and produces F-Droid-compatible output with strict throttling and version-aware redownloading."

## Clarifications

### Session 2026-05-03

- **Q**: How should source credentials (tokens, API keys) be stored and passed to download backends? → **A**: Embedded in the main configuration file alongside package lists, with support for `$VARIABLE` notation to interpolate values from environment variables.
- **Q**: How should old cached versions and removed packages be managed to prevent unbounded disk growth? → **A**: During processing, the system keeps the current version plus a configurable number of previous versions per package. The retention depth is specified in the configuration file. Older versions and artifacts for removed packages are purged from the cache and output directories.
- **Q**: How are throttle intervals scoped — per-source, global, or both, and what does force-refresh bypass? → **A**: Each source has a single `throttle_interval` and a `throttle_policy` (`metadata` or `dumb`). The `metadata` policy checks remote version metadata and skips downloads when the version is unchanged. The `dumb` policy re-downloads unconditionally after the interval. The force-refresh flag bypasses all throttle checks for all sources. PlayStore sources MUST use `dumb` policy because PlayStore version metadata is not supported.
- **Q**: What should happen when an XAPK bundle does not contain a split matching the configured target architecture? → **A**: The system automatically falls back to the closest available architecture. The fallback order for architecture is: exact match → next-closest ABI (e.g., arm64-v8a → armeabi-v7a → armeabi → x86_64 → x86). For density splits, the system merges all available density splits into each architecture APK using the density fallback priority order (exact match → next-lower density). A warning is logged whenever a fallback occurs so the maintainer can review.
- **Q**: How should transient download failures (e.g., dropped connection, temporary 503) be handled? → **A**: Failed downloads are moved to the end of the download queue for one automatic retry. If the retry also fails, the system preserves the previously cached version (if any) and leaves it in the output directory unchanged. The package is checked again on the next application run.
- **Q**: Which download sources are supported, and how is RuStore access provided? → **A**: The system supports five download sources: Google Play Store, RuStore, APKPure, GitHub Releases, and direct web URLs. RuStore is accessed via a fork of the `apkeep` tool (https://github.com/Foxushka/apkeep) that includes RuStore support. The system MUST be designed so that the underlying download backend can be swapped out (e.g., when the RuStore fork is merged into upstream apkeep) without changing the configuration schema or CLI interface.
- **Q**: What is the structure of the per-source configuration, and where should XAPK signing settings live? → **A**: The configuration file groups packages by source. Store sources (PlayStore, RuStore, APKPure) accept a plain package name string. GitHub accepts a dictionary with `repo` (owner/repo) and `mask` (file pattern). WebDL accepts a dictionary with `filename` and `url`. Each source section also contains per-source throttle intervals. XAPK signing settings (keystore path, password, alias, key password) are kept in the main configuration file, not a separate properties file.
- **Q**: Should the XAPK-to-APK conversion be based on the existing reference implementation? → **A**: No. The XAPK-to-APK merging logic must be fully reimplemented from scratch with clean, testable code, and it must be covered by automated tests. The reference implementation is not reused.
- **Q**: What format should the bootstrap-generated signing key use, and what should the bootstrap command do? → **A**: The signing key MUST be generated as a PKCS12 (`.p12`) file. The bootstrap command (`aprepo bootstrap`) creates a template configuration file and generates a random APK signing key stored in the config directory. It MUST fail with a clear error message if the configuration file already exists.
- **Q**: How should version records and throttle timestamps be stored in the cache directory? → **A**: A single state file (`state.yaml`) in the cache root stores version records for all packages. Throttle timestamps are NOT stored in the state file; instead, the system uses the modification time (`mtime`) of the cached APK/XAPK file as the last-check timestamp for throttling decisions.
- **Q**: How should downloads be scheduled — sequentially, concurrently, or mixed? → **A**: Downloads MUST be fully sequential across all sources, with a configurable per-source delay between individual download requests. No parallel downloading is supported.
- **Q**: Should `download` and `process` be chained automatically, and should there be a combined command? → **A**: When no subcommand is provided, `aprepo` runs `download` followed by `process` in sequence as the default behavior. Explicit `download` and `process` subcommands remain available for debugging and rate-limit control. There is no separate `full` command.
- **Q**: How should concurrent invocations of `aprepo` be prevented? → **A**: The system uses the `state.yaml` file as a file lock via an exclusive file lock (e.g., `flock`). If another instance is already running and holds the lock, a new invocation MUST immediately print a clear error message and exit with a non-zero status code.
- **Q**: When should file corruption be detected, and what should happen when a downloaded file is corrupt? → **A**: Immediately after each download completes, the system MUST validate that the file is a well-formed ZIP archive (e.g., via `testzip()` or equivalent). If validation fails, the corrupt file is deleted and the package is moved to the end of the download queue for one retry. If the retry also produces an invalid file, the system follows the same logic as transient failures: it preserves the previously cached version (if any) and leaves it unchanged until the next run.
- **Q**: Should the reimplemented XAPK-to-APK merging logic eliminate external APK tooling dependencies (`apktool`, `zipalign`, `apksigner`), or only reimplement the orchestration layer? → **A**: The system requires `apktool`, `zipalign`, and `apksigner` as external CLI dependencies. The orchestration and merging logic is reimplemented from scratch; the external tools are invoked by the new implementation.
- **Q**: How are package identities scoped — per-source or globally, and what happens if the same package name appears in multiple sources? → **A**: APK package names are globally unique within a configuration. The system MUST validate at startup that no package name appears in more than one source. If a duplicate is found, the system MUST print a clear fatal error and exit with a non-zero status code before any downloads begin.
- **Q**: What are the default cache and output paths, and should the system auto-create missing directories? → **A**: The configuration file MUST specify explicit cache and output paths; there are no hardcoded defaults. On first run, if the configured directories do not exist, the system MUST create them automatically and print a warning that it has done so.
- **Q**: What naming convention should processed APK files in the output directory follow? → **A**: Output APK files MUST be named `package_name_version_architecture.apk` (e.g., `com.example.app_1.2.3_arm64-v8a.apk`). The architecture suffix is the target architecture used during XAPK merging (or `universal` for non-merged APKs).
- **Q**: What is the default list of target architectures, and how is the configuration file structured? → **A**: The default target architecture list is `[arm64-v8a]`. The configuration file is a single YAML file with two top-level keys: `settings` (global configuration) and `sources` (per-source package lists and source-specific options).
- **Q**: How should target density be handled for XAPK merging and output APK generation? → **A**: Density is a global setting in `settings.density` (default `xxhdpi`) that backends may optionally use when downloading APKs directly. For XAPK processing, the system merges all density splits present in the bundle into a single APK per configured architecture using the defined fallback priority order; density is not an output dimension.
- **Q**: Should the system distinguish between permanent (4xx) and transient (5xx/network) download failures when deciding whether to retry? → **A**: No. All download failures are treated uniformly: every failed download gets exactly one retry (moved to the end of the queue). If the retry also fails, the system preserves the previously cached version (if any) and leaves it unchanged until the next run.
- **Q**: Can custom binary paths be configured for external tools (`apkeep`, `gh`, `apktool`, `zipalign`, `apksigner`), or must they be in `$PATH`? → **A**: No custom paths are supported. All external tools MUST be found in the system `$PATH` via standard executable lookup (e.g., `shutil.which` or `which`).
- **Q**: What logging/verbosity levels should the CLI support, and how should output be structured (stdout vs. stderr)? → **A**: The CLI supports a single `--verbose` / `-v` flag. Normal progress and summaries are printed to stdout. Errors and warnings are printed to stderr. When `-v` is passed, debug-level details (per-request URLs, external tool invocation commands, exact retry reasons) are printed to stderr alongside errors.
- **Q**: How should multi-architecture XAPK bundles be processed? → **A**: Each XAPK MUST be repacked into one APK per configured architecture in `settings.architectures`. For each architecture, the system merges the base APK with the appropriate architecture split. If the split for an architecture is missing (even after fallback logic), that architecture is skipped with a warning logged. The output file follows the `package_name_version_architecture.apk` naming convention.
- **Q**: How should the `process` command determine whether a cached file has already been processed to avoid unnecessary re-processing? → **A**: The system compares the cache file's modification time (`mtime`) against the output file's modification time. If the expected output file already exists and its `mtime` is equal to or newer than the cache file's `mtime`, the system skips re-processing that file. Only cache files with an `mtime` newer than their corresponding output file trigger conversion and copying.
- **Q**: How are WebDL packages tracked in `state.yaml`, and how is their Android package name determined for uniqueness and output naming? → **A**: WebDL packages are tracked in `state.yaml` by a stable hash of their `url` field. After each WebDL download completes, the system extracts the true Android package name from the downloaded APK's `AndroidManifest.xml`. The extracted package name is verified against package names from all other sources; if the same package name exists in another source, the system MUST print a clear fatal error and exit with a non-zero status code before any further processing. Output APK files for WebDL downloads use the extracted Android package name, not the configured `filename`.
- **Q**: What happens when a GitHub release has multiple assets that match the configured `mask`? → **A**: The system downloads the first matching asset when sorted alphabetically, logs a warning listing all matched asset names, and proceeds with that single asset. The maintainer is expected to refine the `mask` if the wrong asset is selected.
- **Q**: How are GitHub packages tracked in `state.yaml`, and how is their Android package name determined for uniqueness and output naming? → **A**: GitHub packages are tracked in `state.yaml` by a stable hash of their `repo` and `mask` fields. After each GitHub download completes, the system extracts the true Android package name from the downloaded APK's `AndroidManifest.xml`. The extracted package name is verified against package names from all other sources and against previously processed GitHub/WebDL downloads in the current run; if the same package name exists, the system MUST print a clear fatal error and exit with a non-zero status code before any further processing. Output APK files for GitHub downloads use the extracted Android package name.
- **Q**: How should the cache directory be organized internally — flat, per-source subdirectories, or per-package subdirectories? → **A**: Per-source subdirectories (e.g., `cache_dir/google_play/`, `cache_dir/rustore/`, `cache_dir/apkpure/`, `cache_dir/github/`, `cache_dir/webdl/`). Files are named by package name for store sources and by original asset name / configured filename for GitHub/WebDL. The `state.yaml` file remains at the `cache_dir/` root.
- **Q**: How should the `metadata` throttle policy determine the version for GitHub release downloads? → **A**: The system queries the GitHub API for the latest release of the configured repository. The release `tag_name` (e.g., `v1.2.3`) is used as the version identifier. The system compares the latest release tag against the version stored in `state.yaml`; if the tag has changed, the asset matching the `mask` is downloaded from that release. If the tag is unchanged, the download is skipped.
- **Q**: How should the `metadata` throttle policy fetch version metadata for store sources (e.g., `apkeep`-backed sources) without downloading the file? → **A**: The system invokes the download backend in a metadata-only mode. For `apkeep`-backed sources, this is the `--list-versions` flag, which queries the store API for available versions without downloading APKs. The latest version returned by the backend is compared against the version stored in `state.yaml`; if it has changed, the download proceeds. If it is unchanged, the download is skipped. This keeps the backend abstraction intact per FR-011.
- **Q**: If the configuration file is modified while `aprepo` is running, should the system detect the change, re-read the file, or ignore it? → **A**: The system reads the configuration file once at startup and holds it in memory for the duration of the run. Any changes to the configuration file during a run are ignored. The system respects the configuration as it existed at command invocation time.
- **Q**: Should signing settings be applied to all output APKs, or only to APKs produced by XAPK merging? → **A**: Signing is applied only to APKs produced by XAPK merging. Direct-download APKs from store sources, GitHub, and WebDL retain their original publisher signature and are copied to the output directory unchanged. The signing keystore settings in the configuration file are used exclusively for APKs generated during XAPK-to-APK conversion, where the original signature may be invalidated during repack.
- **Q**: How should the `--package` / `-p` filter option behave for `download` and `process` commands? → **A**: The `--package` option accepts an Android package name (e.g., `-p org.vendor.app`). On `download`, it limits processing to the single matching store-source package; GitHub and WebDL packages are skipped with a warning because their package names are only known after download. On `process`, it limits processing to cached files whose Android package name (extracted from `AndroidManifest.xml`) matches the filter. When no subcommand is provided (default download+process run), `--package` applies to both steps. If no package matches the filter, the command prints a warning and exits with status 0.

### Session 2026-05-04

- **Q**: Should integration tests perform real network downloads against live APIs or use locally stubbed/mocked backends? → **A**: Hybrid approach. Core integration tests in `tests/integrational` use stubbed/mocked backends that serve local APK fixtures, enabling fast, deterministic, offline execution. A separate optional suite (or environment-gated targets) may use live APIs with real credentials to validate end-to-end behavior, but it is not part of the standard `make` target.
- **Q**: For invalid config tests (invalid source name, non-existent app name, etc.), what exit code should `aprepo` return? → **A**: Non-zero exit code (e.g., `1`) for all invalid config errors. The Makefile uses `$?` to detect failure automatically.
- **Q**: The spec says a test succeeds only when there's an APK in `output/` that is a "valid archive." Does this mean a well-formed ZIP (APKs are ZIPs), or must the file also contain a parsable `AndroidManifest.xml`? → **A**: Download phase: a well-formed ZIP archive is sufficient. Process phase: the output APK MUST pass full validation via `aapt2 dump badging` (or equivalent) to verify it is a structurally valid APK with a parsable manifest.
- **Q**: The `make clean` target should remove downloaded artifacts from all test hierarchy. Should it also remove processed output APKs, `state.yaml` files, and any `aprepo`-generated cache metadata, or only the raw downloaded files? → **A**: `make clean` removes all generated artifacts: cache downloads, output APKs, `state.yaml`, and any logs or temp files created by `aprepo` during test runs. Each test run starts from a completely deterministic state.
- **Q**: Should there be a dedicated XAPK fixture test, or is XAPK processing out of scope for integration tests? → **A**: Include a dedicated XAPK fixture test with synthetic base + split APKs. XAPK repack/signing MUST be optional via a `settings.repack_xapk` boolean (default `false`). When disabled, XAPK files are copied to the output directory unchanged. When enabled, the system repacks XAPK bundles into standalone APKs per configured architecture and applies signing if configured.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Download Packages with Throttling and Version Awareness (Priority: P1)

The repository maintainer runs a command to check for and download updates for all configured packages. The system checks each package's current version against the cached version, respects throttling rules to avoid rate limits, and only downloads when a newer version is available or when a force-refresh is requested.

**Why this priority**: Without reliable downloads, the repository contains stale or missing packages. This is the core value proposition of the tool.

**Independent Test**: Can be fully tested by configuring a few packages from different sources, running the download command, and verifying that only new or changed versions are fetched while throttle delays are honored.

**Acceptance Scenarios**:

1. **Given** a configuration with 5 packages across 3 sources and a throttle rule of 24 hours between checks, **When** the maintainer runs the download command twice within the same day, **Then** the second run skips all downloads and completes in under 10 seconds without hitting any remote APIs.

2. **Given** a package with a known cached version of 1.2.3 and the remote source now offering 1.2.4, **When** the maintainer runs the download command after the throttle period has elapsed, **Then** the system downloads version 1.2.4, replaces the cached file, and updates the version record.

3. **Given** a package from a source configured with `throttle_policy: dumb` and a weekly throttle interval, **When** 6 days have passed since the last download, **Then** the system skips the package; **When** 8 days have passed, **Then** the system re-downloads the package unconditionally.

4. **Given** a download command with the `--force` flag enabled, **When** the maintainer runs it before the normal throttle period has elapsed, **Then** the system bypasses throttle checks and attempts downloads for all configured packages immediately.

---

### User Story 2 - Process Cached Files into F-Droid-Compatible APKs (Priority: P2)

The maintainer runs a processing command that transforms everything in the cache directory into clean, standalone APK files in the output directory. XAPK bundles are optionally merged into single APKs based on target device preferences. The output directory contains only files that F-Droid can serve directly.

**Why this priority**: Raw downloads are not directly usable by F-Droid. Processing is required to turn cached artifacts into a valid repository, but it can be run independently of downloads.

**Independent Test**: Can be fully tested by placing a mix of APK and XAPK files in the cache directory, running the process command, and verifying that the output directory contains valid artifacts: standalone APK files and, when `settings.repack_xapk` is `false`, original XAPK files.

**Acceptance Scenarios**:

1. **Given** a cache directory containing 3 APK files and 2 XAPK files, and `settings.architectures` is set to `[arm64-v8a, armeabi-v7a]`, **When** the maintainer runs the process command with `settings.repack_xapk: true`, **Then** the output directory contains 3 standalone APK files from the original APKs and up to 4 APKs from the 2 XAPK files (2 architectures each), all named `package_name_version_architecture.apk`, and no XAPK remnants. If an XAPK is missing a split for one of the configured architectures, that architecture is skipped with a warning. **When** `settings.repack_xapk` is `false`, **Then** the 3 APK files are copied unchanged and the 2 XAPK files are copied unchanged to the output directory.

2. **Given** a cache directory containing a corrupted or incomplete download file, **When** the maintainer runs the process command, **Then** the system logs the corruption, skips the file, and continues processing the remaining valid files without crashing.

3. **Given** a cache directory with files already present in the output directory, **When** the maintainer runs the process command, **Then** the system compares each cache file's `mtime` against its corresponding output file's `mtime`. If the output file exists and its `mtime` is equal to or newer than the cache file's `mtime`, the system skips re-processing. Only cache files with an `mtime` newer than their corresponding output file trigger conversion and copying.

4. **Given** a cache directory with files already present in the output directory, **When** the maintainer runs the process command with the `--force` flag, **Then** the system bypasses the mtime comparison and re-processes all valid cache files unconditionally, regenerating every output APK.

---

### User Story 3 - Manage Repository via Simple CLI Commands (Priority: P3)

The maintainer interacts with the entire system through a small set of CLI commands with a configuration file. Commands are intuitive, accept a configuration path, and provide clear feedback about what is happening.

**Why this priority**: A usable interface is important, but it is a delivery layer over the core download and processing logic. It can be developed and refined after the core engine works.

**Independent Test**: Can be fully tested by invoking each CLI command with valid and invalid arguments and verifying appropriate behavior, help text, and exit codes.

**CLI Interface Reference**:

```
Usage: aprepo [OPTIONS] [COMMAND]

When no COMMAND is provided, the default behavior runs download followed by
process in sequence.

Commands:
  bootstrap   Create a template configuration file and a random PKCS12 signing keystore
  download    Check remote sources and download new or updated packages
  process     Convert cached APK/XAPK files into clean F-Droid-compatible APKs
  help        Print this message or the help of the given subcommand(s)

Options:
  -c, --config <FILE>    Path to the YAML configuration file
  -v, --verbose          Emit debug-level details to stderr
  -h, --help             Print help
  -V, --version          Print version

Command-specific options:
  aprepo bootstrap --config <FILE>
      Creates a template config and a .p12 keystore at the specified path.
      Fails if the configuration file already exists.

  aprepo [OPTIONS] [--force] [--package <NAME>] [-v]
      Default behavior: download then process.
      --force            Apply force to both download and process steps
      -p, --package <NAME> Apply package filter to both steps

  aprepo download --config <FILE> [--force] [--package <NAME>] [-v]
      --force            Bypass throttle checks and re-download all packages
      -p, --package <NAME> Download only the matching store-source package;
                           GitHub/WebDL are skipped with a warning

  aprepo process --config <FILE> [--force] [--package <NAME>] [-v]
      --force            Bypass mtime checks and re-process all cache files
      -p, --package <NAME> Process only cached files matching the given
                           Android package name
```

**Acceptance Scenarios**:

1. **Given** a valid configuration file path, **When** the maintainer runs `download` with the configuration flag, **Then** the system loads the configuration, executes downloads, and prints a summary of skipped, downloaded, and failed packages to stdout. Errors and warnings are printed to stderr.

2. **Given** a missing or malformed configuration file, **When** the maintainer runs any command, **Then** the system prints a clear error message explaining the configuration problem and exits with a non-zero status.

3. **Given** a running download command, **When** the maintainer interrupts it (e.g., Ctrl+C), **Then** the system stops gracefully, preserves already-downloaded files and metadata, and does not leave the cache in an inconsistent state.

4. **Given** that no configuration file exists at the target path, **When** the maintainer runs the `bootstrap` command, **Then** the system creates a template YAML configuration file with top-level `settings` and `sources` keys, including default cache/output paths, retention depth, architecture list, and placeholder source sections, and a random PKCS12 signing keystore, and prints the path to both.

5. **Given** that a configuration file already exists at the target path, **When** the maintainer runs the `bootstrap` command, **Then** the system prints a clear error message stating the file already exists and exits with a non-zero status without overwriting anything.

6. **Given** a valid configuration with packages to update, **When** the maintainer runs `aprepo` with no subcommand, **Then** the system first executes `download` and then `process` automatically. The `process` step evaluates all cache files using the standard mtime-based skip logic, not only packages that were downloaded in the preceding step. If `download` completes with errors but some packages succeeded, **Then** `process` still runs. If `download` fails for every configured package, **Then** the system stops without running `process`.

7. **Given** a valid configuration where all packages are already cached and processed, **When** the maintainer runs `aprepo --force` with no subcommand, **Then** the `download` step re-downloads all packages unconditionally (bypassing throttle checks), and the `process` step re-processes all cache files unconditionally (bypassing mtime checks), regenerating all output APKs.

8. **Given** a valid configuration with 5 packages across 3 store sources, **When** the maintainer runs `download --package org.vendor.app`, **Then** the system downloads only the package `org.vendor.app`, skips all other packages without hitting their remote APIs, and prints a summary showing 1 processed and 4 skipped.

9. **Given** a cache directory containing 3 APK files for different packages, **When** the maintainer runs `process --package com.example.app`, **Then** the system processes only the cached file for `com.example.app`, skips the other two without extracting or copying them, and prints a summary showing 1 processed and 2 skipped.

10. **Given** a configuration where the package `com.missing.app` is not configured in any source, **When** the maintainer runs `download --package com.missing.app`, **Then** the system prints a warning that no matching package was found and exits with status 0 without attempting any downloads.

---

### Edge Cases

- What happens when a download fails or produces a corrupt file? After each download, the system validates the file as a well-formed ZIP archive. If validation fails, the corrupt file is deleted and the package is moved to the end of the download queue for one retry. All download failures (network error, HTTP error, or corrupt file) are treated uniformly and get exactly one retry. If the retry also fails, the system preserves the previously cached version (if any) and leaves it in the output directory unchanged. The package is checked again on the next application run.
- What happens when the disk is full during download? The system detects the write failure, aborts the current download, cleans up the partial file, and reports the error without corrupting existing cache contents.
- What happens when two packages share the same package name across different sources? This is a fatal configuration error. The system detects duplicates at startup, prints a clear error listing the conflicting sources and package names, and exits with a non-zero status code before any downloads begin.
- What happens when an XAPK merge fails midway? (Only applicable when `settings.repack_xapk` is `true`.) The system cleans up any temporary unpack directories and does not place a partial APK in the output directory.
- What happens when the configured target architecture is not present in an XAPK bundle? The system automatically falls back to the next-closest available ABI using the defined priority order (arm64-v8a → armeabi-v7a → armeabi → x86_64 → x86). A warning is logged for each fallback so the maintainer can review. If even the fallback architecture is missing, that architecture variant is skipped entirely and a warning is logged. For density splits, the system merges all available density splits into each architecture APK using the density fallback priority order (exact match → next-lower density). Density is not an output dimension.
- What happens when the configuration changes mid-cycle (e.g., a package is removed)? The system respects the current configuration at command invocation time; previously cached files for removed packages are ignored during processing and are purged along with any versions exceeding the configured retention depth.
- What happens when a required external tool is missing at startup? For downloads: if `apkeep` or `gh` is missing, the system skips all packages for the affected source, logs a clear error, and continues with other sources. For processing: if `settings.repack_xapk` is `true` and `apktool` or `zipalign` is missing, the system aborts the `process` command with a clear fatal error and a non-zero exit code. If `apksigner` is missing but signing is enabled, the system logs a warning and skips signing for all APKs produced from that XAPK. When `settings.repack_xapk` is `false`, `apktool`, `zipalign`, and `apksigner` are not required.
- What happens when a second `aprepo` instance is started while another is already running? The system attempts to acquire an exclusive file lock on `state.yaml`. If the lock is already held, the second invocation prints a clear error message stating that another instance is running and exits immediately with a non-zero status code.
- What happens when a WebDL or GitHub download's extracted Android package name matches a package from another source? This is a fatal error. After the download completes, the system extracts the package name from the APK's `AndroidManifest.xml`. If that name already exists in another source (or in a previously processed WebDL or GitHub entry during the same run), the system prints a clear error listing the conflicting entries and exits with a non-zero status code before processing that package.
- What happens when a GitHub release has multiple assets matching the configured `mask`? The system selects the first matching asset when sorted alphabetically, logs a warning listing all matched asset names, and proceeds with that single asset. The maintainer is expected to refine the `mask` if the wrong asset is selected.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST read package definitions and source settings from a user-supplied configuration file.
- **FR-002**: The system MUST support multiple download sources simultaneously within a single configuration.
- **FR-003**: The system MUST track the last downloaded version for each package and only re-download when the remote version differs.
- **FR-003a**: After each download completes, the system MUST validate that the downloaded file is a well-formed ZIP archive (e.g., via `testzip()` or equivalent). If validation fails, the corrupt file MUST be deleted and the package MUST be moved to the end of the download queue for one retry. Every failed download (network error, corrupt file, HTTP error, or otherwise) gets exactly one retry by being moved to the end of the queue. If the retry also fails, the system MUST preserve the previously cached version (if any) and leave it in the output directory unchanged until the next application run.
- **FR-003c**: The system MUST process downloads fully sequentially — one package at a time — with a configurable per-source delay between individual download requests.
- **FR-003b**: For direct web URL downloads where semantic version metadata is unavailable, the system MUST perform a HEAD request and generate a version identifier by hashing the `ETag`, `Last-Modified`, and `Content-Length` response headers. A change in any of these headers MUST trigger a re-download.
- **FR-003d**: For GitHub releases, if multiple release assets match the configured `mask`, the system MUST select the first matching asset when sorted alphabetically, log a warning listing all matched asset names, and proceed with that single asset.
- **FR-003e**: For GitHub releases configured with `throttle_policy: metadata`, the system MUST query the GitHub API for the latest release of the configured repository and use the release `tag_name` as the version identifier. The system MUST compare the latest release tag against the version stored in `state.yaml` and only download the matching asset if the tag has changed.
- **FR-004**: The system MUST enforce a per-source throttle interval between download attempts. Each source MUST specify a `throttle_policy` of either `metadata` or `dumb`. With `metadata` policy, the system MUST check remote version metadata and skip the download if the version has not changed since the last check. With `dumb` policy, the system MUST re-download unconditionally after the throttle interval has elapsed since the last download.
- **FR-005**: The system MUST validate at startup that PlayStore (`google_play`) sources are configured with `throttle_policy: dumb`. If a PlayStore source is configured with `throttle_policy: metadata`, the system MUST print a clear fatal error and exit with a non-zero status code, because PlayStore version metadata is not supported.
- **FR-006**: The system MUST allow the maintainer to force a download cycle that bypasses all throttle checks for all configured sources via a `--force` command-line flag. When `--force` is passed, the system MUST re-download all configured packages regardless of throttle state.
- **FR-006b**: The CLI MUST accept a `--package` / `-p` flag that accepts an Android package name. When provided to the `download` step, the system MUST limit downloads to the single matching package from store sources (`google_play`, `rustore`, `apkpure`). GitHub and WebDL packages MUST be skipped with a warning when `--package` is used because their package names are unknown until after download. If no configured package matches the filter, the system MUST print a warning and exit with status 0.
- **FR-007**: The system MUST place all raw downloads and internal state into a designated cache directory and MUST manage retention so that only the current version plus a configurable number of previous versions per package are kept; older versions MUST be purged during processing. The cache and output paths MUST be explicitly defined in the configuration file. If a configured directory does not exist at startup, the system MUST create it automatically and print a warning.
- **FR-008**: The system MUST provide a separate processing step that transforms cache contents into a clean output directory. Direct-download APK files MUST be copied unchanged. When `settings.repack_xapk` is `true`, XAPK bundles MUST be repacked into standalone APK files. When `settings.repack_xapk` is `false`, XAPK files MUST be copied to the output directory unchanged. Output APK files MUST be named `package_name_version_architecture.apk`, where `version` is the `versionName` extracted from the APK's `AndroidManifest.xml`, and `architecture` is the target ABI used during XAPK merging (or `universal` for non-merged APKs).
- **FR-009**: When `settings.repack_xapk` is `true`, the system MUST repack each cached XAPK bundle into one standalone APK per configured architecture in `settings.architectures`. For each architecture, the system MUST merge the base APK with the appropriate architecture split. If the split for an architecture is missing (even after fallback), that architecture MUST be skipped and a warning MUST be logged. For density splits, the system MUST merge all available density splits into each architecture APK using the density fallback priority order. When `settings.repack_xapk` is `false`, the system MUST copy XAPK files to the output directory unchanged.
- **FR-009a**: When `settings.repack_xapk` is `true` and XAPK processing produces APKs, the system MUST read signing keystore settings from the main configuration file and apply them to each generated APK. Direct-download APKs (from store sources, GitHub, or WebDL) MUST be copied to the output directory with their original publisher signature intact; signing settings MUST NOT be applied to them. If signing settings are missing or invalid for an XAPK that requires signing, the system MUST log a warning and skip signing for all APKs produced from that XAPK. When `settings.repack_xapk` is `false`, signing settings are not used.
- **FR-009b**: The XAPK-to-APK merging logic MUST be fully reimplemented from scratch; the reference implementation is not reused. The reimplemented logic MUST be covered by automated tests.
- **FR-010**: The system MUST skip any cache files that fail validation during processing and continue with valid files. Files are already validated as well-formed ZIP archives at download time; processing validation is a secondary safety check.
- **FR-010a**: The CLI MUST accept a `--force` flag for the `process` step. When provided, the system MUST bypass the mtime comparison check and re-process all valid cache files, regenerating output APKs unconditionally.
- **FR-010b**: The CLI MUST accept a `--package` / `-p` flag for the `process` step. When provided, the system MUST limit processing to cached files whose Android package name (extracted from `AndroidManifest.xml`) matches the filter. If no cached file matches the filter, the system MUST print a warning and exit with status 0.
- **FR-011**: The system MUST support swapping the underlying download backend without changing the package configuration format.
- **FR-012**: The system MUST print clear, actionable status messages for each operation (download skipped, downloaded, failed; processed, skipped, error). Normal progress and summaries MUST be written to stdout; errors and warnings MUST be written to stderr.
- **FR-012a**: The CLI MUST accept a `--verbose` / `-v` flag. When provided, the system MUST emit debug-level details (per-request URLs, external tool invocation commands, exact retry reasons) to stderr in addition to normal output.
- **FR-013**: The system MUST handle interruption gracefully, leaving cache and output directories in a consistent state.
- **FR-014**: At startup, the system MUST verify that all required external tools for the current command are installed and accessible by looking them up in the system `$PATH` via standard executable resolution. For downloads, store sources require the `apkeep` binary and GitHub requires the `gh` CLI; if a download tool is missing, the system MUST skip all packages for that source, log a clear error, and continue processing other sources. For XAPK processing, `apktool` and `zipalign` are required; if signing is enabled, `apksigner` is also required. If a required processing tool is missing, the system MUST abort the `process` command with a clear fatal error and a non-zero exit code. Custom binary paths are not supported.
- **FR-015**: The system MUST provide a `bootstrap` command that generates a template YAML configuration file with top-level `settings` and `sources` keys, including default values for cache/output paths, retention depth, architecture list, and placeholder source sections, and creates a random PKCS12 signing keystore in the config directory. The bootstrap command MUST fail with a clear error and a non-zero exit code if the target configuration file already exists.
- **FR-016**: When no subcommand is provided, the system MUST execute `download` followed by `process` in sequence as the default behavior. The `process` step operates on all cache contents using the standard mtime-based skip logic, not only on packages that were downloaded in the preceding `download` step. If `download` fails for every configured package (total failure), the system MUST stop without running `process`. If `download` succeeds for at least one package (partial failure), the system MUST continue to `process`. The default run MUST accept the same flags as the individual `download` and `process` commands (including `--force`, `--verbose`, and `--package`).
- **FR-017**: The system MUST acquire an exclusive file lock on `state.yaml` at startup. If the lock cannot be acquired because another instance is running, the system MUST immediately print a clear error message and exit with a non-zero status code.
- **FR-018**: The system MUST validate at startup that all configured package names from store sources (`google_play`, `rustore`, `apkpure`) are globally unique across those sources. If a duplicate package name is found among store sources, the system MUST print a clear fatal error and exit with a non-zero status code before any downloads begin.
- **FR-018a**: After each WebDL or GitHub download completes, the system MUST extract the Android package name from the downloaded APK's `AndroidManifest.xml`. The extracted package name MUST be verified against all package names from store sources and against the extracted package names of all previously processed WebDL and GitHub downloads in the current run. If a duplicate package name is found, the system MUST print a clear fatal error listing the conflicting sources and exit with a non-zero status code before any further processing of that package.

### Key Entities

- **Package**: A mobile application identified by its package name. For store sources (`google_play`, `rustore`, `apkpure`), the package name is explicitly configured. For GitHub and WebDL sources, the package name is extracted from the downloaded APK's `AndroidManifest.xml`. Package names MUST be globally unique within a configuration; the same name MUST NOT appear in multiple sources. Each package is associated with exactly one download source and tracked by version.
- **Configuration**: A single YAML file with two top-level keys: `settings` (global configuration) and `sources` (per-source package lists and source-specific options). Global settings include explicit cache and output directory paths, cache retention depth, target architecture list, and optional XAPK signing keystore details. Per-source sections define packages, throttle intervals, request delays, and optional credentials. The signing keystore MUST be in PKCS12 (`.p12`) format. Credential and keystore values support `$VARIABLE` notation to interpolate environment variables at runtime. See `Configuration File Format` below for the full schema.
- **Cache Directory**: The working storage for raw downloads (APK, XAPK) and internal metadata. Raw downloads are organized into per-source subdirectories (e.g., `cache_dir/google_play/`, `cache_dir/rustore/`, `cache_dir/apkpure/`, `cache_dir/github/`, `cache_dir/webdl/`). Version records are stored in a single `state.yaml` file in the cache root. Throttle timestamps are derived from the modification time (`mtime`) of cached files.
- **Output Directory**: The final destination for processed, standalone APK files ready for consumption by F-Droid.
- **Throttle Record**: Implicitly derived from the modification time (`mtime`) of the cached file for each package. The system uses `mtime` together with the source's `throttle_policy` (`metadata` or `dumb`) and `throttle_interval` to decide whether to skip or proceed with a download. No separate timestamp storage is required.

### Configuration File Format

The configuration file is a single YAML file with two top-level keys: `settings` and `sources`.

```yaml
settings:
  cache_dir: "./cache"
  output_dir: "./output"
  retention_depth: 1
  architectures:
    - arm64-v8a
  repack_xapk: false
  sign:
    enabled: false
    keystore_file: "./signing.p12"
    keystore_password: "$KEYSTORE_PASSWORD"
    key_alias: "aprepo"
    key_password: "$KEY_PASSWORD"

sources:
  google_play:
    throttle_policy: "dumb"
    throttle_interval: "24h"
    delay_between_requests: "2s"
    token: "$PLAYSTORE_TOKEN"
    packages:
      - com.example.app
  rustore:
    throttle_policy: "metadata"
    throttle_interval: "24h"
    delay_between_requests: "2s"
    packages:
      - ru.sberbankmobile
  apkpure:
    throttle_policy: "metadata"
    throttle_interval: "24h"
    delay_between_requests: "2s"
    packages:
      - com.example.app
  github:
    throttle_policy: "metadata"
    throttle_interval: "24h"
    delay_between_requests: "2s"
    token: "$GITHUB_TOKEN"
    packages:
      - repo: "topjohnwu/Magisk"
        mask: "Magisk*.apk"
  webdl:
    throttle_policy: "metadata"
    throttle_interval: "24h"
    delay_between_requests: "2s"
    packages:
      - filename: "custom-app.apk"
        url: "https://example.com/app.apk"
```

**`settings` (global settings)**:
- `cache_dir` (string, required): Absolute or relative path to the cache directory.
- `output_dir` (string, required): Absolute or relative path to the output directory.
- `retention_depth` (integer, default `1`): Number of previous versions to keep per package in addition to the current version.
- `architectures` (list of strings, default `[arm64-v8a]`): Target ABIs for XAPK merging and output APK naming. Supported values: `arm64-v8a`, `armeabi-v7a`, `armeabi`, `x86_64`, `x86`.
- `density` (string, default `xxhdpi`): Preferred screen density for backends that support density-specific downloads. Supported values: `xxxhdpi`, `xxhdpi`, `xhdpi`, `hdpi`, `mdpi`, `ldpi`, `nodpi`, `tvdpi`.
- `repack_xapk` (boolean, default `false`): When `true`, the system repacks cached XAPK bundles into standalone APKs per configured architecture and applies signing if configured. When `false`, XAPK files are copied to the output directory unchanged.
- `sign` (object, optional):
  - `enabled` (boolean, default `false`)
  - `keystore_file` (string, required if enabled): Path to the PKCS12 keystore.
  - `keystore_password` (string, required if enabled): Supports `$VARIABLE` interpolation.
  - `key_alias` (string, required if enabled)
  - `key_password` (string, required if enabled): Supports `$VARIABLE` interpolation.

**`sources` (per-source sections)** — each source key is optional; only configured sources are processed:
- `google_play`:
  - `throttle_policy` (string, default `dumb`): Throttle mode. MUST be `dumb` for PlayStore; `metadata` is unsupported and causes a fatal startup error.
  - `throttle_interval` (string, default `24h`): Minimum time between download attempts for this source. Format: `<number><unit>` where unit is `h` (hours) or `d` (days).
  - `delay_between_requests` (string, default `2s`): Delay between consecutive download requests for this source.
  - `token` (string, optional): Play Store access token. Supports `$VARIABLE` interpolation.
  - `packages` (list of strings): Package names to download.
- `rustore`: Same fields as `google_play`, except `throttle_policy` may be `metadata` or `dumb`.
- `apkpure`: Same fields as `google_play`, except `throttle_policy` may be `metadata` or `dumb`.
- `github`:
  - `throttle_policy`, `throttle_interval`, `delay_between_requests`: Same semantics as store sources.
  - `token` (string, optional): GitHub personal access token. Supports `$VARIABLE` interpolation.
  - `packages` (list of objects):
    - `repo` (string, required): GitHub repository in `owner/repo` format.
    - `mask` (string, required): File pattern for matching release assets (e.g., `*.apk`). If multiple assets in a single release match the mask, the system selects the first match sorted alphabetically and logs a warning listing all matched asset names.
- `webdl`:
  - `throttle_policy`, `throttle_interval`, `delay_between_requests`: Same semantics as other sources.
  - `packages` (list of objects):
    - `filename` (string, required): Local filename for storing the downloaded file in the cache directory. The file extension SHOULD be `.apk`.
    - `url` (string, required): Direct download URL. This URL is used as the stable identifier for tracking the package in `state.yaml` (via a hash of the URL). After download, the actual Android package name is extracted from the APK's manifest for uniqueness validation and output naming.

**Validation rules**:
- The system MUST reject configurations where the same package name appears in multiple store sources (`google_play`, `rustore`, `apkpure`) (fatal error at startup). GitHub and WebDL package uniqueness are checked post-download by extracting the Android package name from the downloaded APK.
- The system MUST reject configurations where `settings.cache_dir` or `settings.output_dir` are missing or empty.
- The system MUST accept but warn when `settings.sign.enabled` is `true` but any signing field is missing.
- The system MUST accept but warn when `settings.repack_xapk` is `false` but `settings.sign.enabled` is `true` (signing has no effect when repacking is disabled).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A download cycle with no updates completes in under 30 seconds for a repository of 50 packages when throttle rules are respected.
- **SC-002**: 100% of packages with available version metadata are re-downloaded only when the version changes; no redundant downloads occur between version changes.
- **SC-003**: Packages configured with `throttle_policy: dumb` are re-downloaded no more frequently than the configured throttle interval.
- **SC-004**: After processing, 100% of files in the output directory are valid artifacts: standalone APK files for direct downloads and repacked XAPKs; when `settings.repack_xapk` is `false`, original XAPK files are also permitted. No ZIP-wrapped artifacts or partial files remain.
- **SC-005**: The system recovers gracefully from interruption: after a forced stop during download or processing, the next invocation produces the same result as if the previous run had completed successfully.
- **SC-006**: Adding support for a new download source requires changes only to the backend integration layer, with zero modifications to the configuration schema or CLI interface.
- **SC-007**: Integration tests in `tests/integrational` verify that: (a) invalid configurations produce a non-zero exit code; (b) each download source fixture produces a well-formed ZIP in the cache directory; (c) the process phase produces output APKs that pass `aapt2 dump badging` validation; (d) both direct-token and `$VARIABLE` syntax for credentials are exercised and function correctly; (e) a synthetic XAPK fixture with base + split APKs validates both `repack_xapk: true` (repacked APKs in output) and `repack_xapk: false` (XAPK copied unchanged) paths.

## Assumptions

- The maintainer has a valid YAML configuration file with top-level `settings` and `sources` keys, correct package identifiers, and credentials for each source. Credential fields may reference environment variables via `$VARIABLE` notation.
- The system runs on a host with sufficient disk space for both cache and output directories.
- Network connectivity is available during download commands; processing can run offline.
- XAPK repacking is controlled by `settings.repack_xapk`. When enabled, target architecture and screen density preferences, plus optional XAPK signing keystore details, are read from the configuration file. The signing keystore is in PKCS12 (`.p12`) format.
- The system verifies at startup that `apkeep` (including the RuStore fork) is installed and accessible for store downloads, and that the `gh` CLI is installed and accessible for GitHub downloads. If either is missing, packages for that source are skipped. All external tools are resolved via the system `$PATH`; custom binary paths are not supported.
- When `settings.repack_xapk` is `true`, XAPK processing requires `apktool`, `zipalign`, and `apksigner` to be installed and accessible in the system `$PATH`. The system checks for these tools at the start of the `process` command. If `apktool` or `zipalign` is missing, the system aborts the `process` command with a clear fatal error and a non-zero exit code. If `apksigner` is missing but signing is enabled, the system logs a warning and skips signing for all APKs produced from that XAPK; if signing is disabled, `apksigner` is not required. When `settings.repack_xapk` is `false`, these tools are not required.
- Direct web URL downloads require only standard HTTP client capabilities (no external CLI dependencies).
- F-Droid repository generation (indexing, signing) is handled by external tools; this application only produces the APK files that feed into that pipeline.
- All downloads must be direct APK or XAPK files; ZIP-wrapped artifacts are not supported.
- Publishing or distributing the output directory (e.g., via WebDAV, rsync, or other transport) is handled by external tools outside the scope of this application.
