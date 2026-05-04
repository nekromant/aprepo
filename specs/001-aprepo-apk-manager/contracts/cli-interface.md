# CLI Interface Contract

## Entrypoint

```
aprepo [OPTIONS] [COMMAND]
```

When no `COMMAND` is provided, the default behavior runs `download` followed by `process` in sequence.

## Global Options

| Flag | Short | Argument | Description |
|------|-------|----------|-------------|
| `--config` | `-c` | `<FILE>` | Path to YAML configuration file (required for all commands) |
| `--verbose` | `-v` | — | Emit debug-level details to stderr |
| `--help` | `-h` | — | Print help |
| `--version` | `-V` | — | Print version |

## Commands

### `bootstrap`

```
aprepo bootstrap --config <FILE>
```

**Behavior**:
- Creates a template YAML configuration file at the specified path.
- Generates a random PKCS12 (`.p12`) signing keystore in the same directory as the config file.
- **Fails** with a clear error and non-zero exit code if the configuration file already exists.

**Output**:
- Prints paths to the created config file and keystore to stdout.

### `download`

```
aprepo download --config <FILE> [--force] [--package <NAME>] [-v]
```

**Behavior**:
- Loads configuration, validates it (uniqueness, PlayStore policy).
- Acquires exclusive lock on `state.yaml`.
- Checks each configured package against throttle rules and version metadata.
- Downloads new or updated packages sequentially with per-source delays.
- Validates each downloaded file as a well-formed ZIP archive.
- Retries failed downloads exactly once (moved to end of queue).
- Updates `state.yaml` with new version records.
- Releases lock on completion.

**Command Options**:

| Flag | Short | Description |
|------|-------|-------------|
| `--force` | — | Bypass all throttle checks, re-download all packages, and re-test per-architecture backend capability for store sources |
| `--package` | `-p` | Limit to single matching store-source package (GitHub/WebDL skipped with warning) |

**Exit Codes**:
- `0`: All operations completed (some packages may have been skipped).
- `1`: Fatal error (config invalid, duplicate package, lock held, PlayStore policy violation).
- `0` (with warning): No package matches `--package` filter.

### `process`

```
aprepo process --config <FILE> [--force] [--package <NAME>] [-v]
```

**Behavior**:
- Loads configuration.
- Scans cache directory for all APK and XAPK files.
- Compares cache file `mtime` against corresponding output file `mtime`.
- Processes files where cache is newer, or all files if `--force`.
- For APK files: copies to output directory unchanged.
- For XAPK files:
  - If `settings.repack_xapk == true`: repacks into one APK per configured architecture, optionally signs.
  - If `settings.repack_xapk == false`: copies to output directory unchanged.
- Applies retention: purges old versions and removed packages from cache and output.

**Command Options**:

| Flag | Short | Description |
|------|-------|-------------|
| `--force` | — | Bypass mtime checks and re-process all valid cache files |
| `--package` | `-p` | Limit to cached files matching the given Android package name |

**Exit Codes**:
- `0`: Processing completed (some files may have been skipped).
- `1`: Fatal error (missing required external tool, config invalid).

### Default Run (no subcommand)

```
aprepo --config <FILE> [--force] [--package <NAME>] [-v]
```

**Behavior**:
1. Executes `download` step.
2. If `download` fails for **every** configured package (total failure): stop without running `process`.
3. If `download` succeeds for **at least one** package (partial failure): continue to `process`.
4. `process` evaluates **all** cache contents using standard mtime-based skip logic.

**Flags**: Same as `download` and `process` (applied to both steps).

## Output Streams

- **stdout**: Normal progress, summaries, bootstrap paths.
- **stderr**: Errors, warnings, debug details (when `-v`).

## Summary Format (stdout)

At the end of each command, print a summary line per operation type:

```
Download: 12 skipped, 3 downloaded, 1 failed
Process: 10 skipped, 5 processed, 0 errors
```

## Error Message Format (stderr)

All error messages MUST be clear and actionable:

```
ERROR: Configuration file not found: /path/to/config.yaml
ERROR: Duplicate package 'com.example.app' found in sources: google_play, rustore
ERROR: PlayStore source 'google_play' must use throttle_policy: dumb (found: metadata)
ERROR: Another instance is already running (lock held on /path/to/cache/state.yaml)
```
