# Quickstart Guide: APRepo APK Download and Processing Manager

## Prerequisites

- Rust 1.78+ with Cargo
- `apkeep` binary (including RuStore fork) in `$PATH`
- `gh` CLI in `$PATH` (for GitHub source)
- `apktool`, `zipalign`, `apksigner` in `$PATH` (only if `repack_xapk: true`)
- `aapt2` (optional, for integration test fixture generation)
- Linux x86_64 host

## Build

```bash
cargo build --release
```

The binary is produced at `target/release/aprepo`.

## Bootstrap Configuration

Create a template configuration and signing keystore:

```bash
./target/release/aprepo bootstrap --config ./aprepo.yaml
```

This creates:
- `./aprepo.yaml` — template configuration file
- `./signing.p12` — random PKCS12 keystore (if signing is enabled in template)

**Note**: If `./aprepo.yaml` already exists, the command fails with a clear error.

## Edit Configuration

Open `aprepo.yaml` and customize:

```yaml
settings:
  cache_dir: "./cache"
  output_dir: "./output"
  retention_depth: 1
  architectures:
    - arm64-v8a
  repack_xapk: false   # set to true to enable XAPK repacking
  sign:
    enabled: false
    keystore_file: "./signing.p12"
    keystore_password: "$KEYSTORE_PASSWORD"
    key_alias: "aprepo"
    key_password: "$KEY_PASSWORD"

sources:
  google_play:
    throttle_policy: dumb
    throttle_interval: 24h
    delay_between_requests: 2s
    token: "$PLAYSTORE_TOKEN"
    packages:
      - com.example.app
  github:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    token: "$GITHUB_TOKEN"
    packages:
      - repo: "owner/repo"
        mask: "*.apk"
        arch_masks:
          arm64-v8a: "*-arm64.apk"
          armeabi-v7a: "*-armeabi-v7a.apk"
  webdl:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    packages:
      - filename: "custom.apk"
        url: "https://example.com/app.apk"
        arch_files:
          arm64-v8a:
            filename: "custom-arm64.apk"
            url: "https://example.com/app-arm64.apk"
```

Set environment variables for any `$VARIABLE` tokens:

```bash
export PLAYSTORE_TOKEN="your_token_here"
export GITHUB_TOKEN="your_token_here"
```

## Run

### Default (download + process)

```bash
./target/release/aprepo --config ./aprepo.yaml
```

### Download only

```bash
./target/release/aprepo download --config ./aprepo.yaml
```

### Process only

```bash
./target/release/aprepo process --config ./aprepo.yaml
```

### Force refresh

```bash
./target/release/aprepo --config ./aprepo.yaml --force
```

### Single package

```bash
./target/release/aprepo download --config ./aprepo.yaml --package com.example.app
```

## Output

After a successful run:

- `./cache/` — raw downloaded APK/XAPK files and `state.yaml`
- `./output/` — processed APK files (or XAPK copies if `repack_xapk: false`)

## Integration Tests

```bash
cd tests/integrational
make        # run all integration tests
make clean  # remove all generated artifacts
```

Test structure:

```
tests/integrational/
├── Makefile
├── invalid/          # Invalid config fixtures
├── sources/          # Per-source valid config fixtures
│   ├── google_play/
│   ├── rustore/
│   ├── apkpure/
│   ├── github/
│   └── webdl/
├── xapk/             # XAPK repack on/off fixtures
└── fixtures/
    └── generate.sh   # Synthetic APK fixture generator
```

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `ERROR: Another instance is already running` | `state.yaml` is locked by another `aprepo` process | Wait for the other process to finish, or kill it |
| `ERROR: Configuration file not found` | Wrong `--config` path | Verify the path to `aprepo.yaml` |
| `WARNING: PlayStore source must use dumb policy` | `google_play.throttle_policy` is `metadata` | Change to `dumb` |
| `WARNING: Missing signing field` | `sign.enabled: true` but a signing sub-field is empty | Fill in all signing fields or disable signing |
| Downloads skipped unexpectedly | Throttle interval not yet elapsed | Check `mtime` of cached files or use `--force` |
