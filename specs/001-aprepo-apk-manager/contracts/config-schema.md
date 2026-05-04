# Configuration Schema Contract

## File Format

A single YAML file with two top-level keys: `settings` and `sources`.

## Top-Level Structure

```yaml
settings:
  cache_dir: "./cache"
  output_dir: "./output"
  retention_depth: 1
  architectures:
    - arm64-v8a
  density: xxhdpi
  repack_xapk: false
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
  rustore:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    packages:
      - ru.sberbankmobile
  apkpure:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
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

## `settings` Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `cache_dir` | `string` | Yes | — | Absolute or relative path to cache directory. Auto-created if missing. |
| `output_dir` | `string` | Yes | — | Absolute or relative path to output directory. Auto-created if missing. |
| `retention_depth` | `integer` | No | `1` | Number of previous versions to keep per package. |
| `architectures` | `list[string]` | No | `["arm64-v8a"]` | Target ABIs for downloads and XAPK repacking. Store backends are invoked once per architecture. GitHub/WebDL per-architecture configs are filtered by this list. |
| `density` | `string` | No | `"xxhdpi"` | Preferred screen density. |
| `repack_xapk` | `boolean` | No | `false` | When `true`, repack XAPK bundles into APKs. When `false`, copy XAPK unchanged. |
| `sign` | `object` | No | `null` | Signing configuration for repacked APKs. |

### `sign` Sub-fields

| Field | Type | Required if `sign.enabled` | Description |
|-------|------|---------------------------|-------------|
| `enabled` | `boolean` | N/A | Default `false`. |
| `keystore_file` | `string` | Yes | Path to PKCS12 (`.p12`) keystore. |
| `keystore_password` | `string` | Yes | Supports `$VARIABLE` interpolation. |
| `key_alias` | `string` | Yes | Key alias inside the keystore. |
| `key_password` | `string` | Yes | Supports `$VARIABLE` interpolation. |

## `sources` Sections

Each source key is optional. Only configured sources are processed.

### Common Source Fields

All source sections share these fields:

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `throttle_policy` | `string` | No | varies | `dumb` or `metadata` |
| `throttle_interval` | `string` | No | `24h` | Format: `<number>h` or `<number>d` |
| `delay_between_requests` | `string` | No | `2s` | Format: `<number>s` or `<number>m` |

### Store Sources (`google_play`, `rustore`, `apkpure`)

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `token` | `string` | No | `null` | Source access token. Supports `$VARIABLE`. |
| `packages` | `list[string]` | Yes | `[]` | Android package names to download. |

**Constraints**:
- `google_play`: `throttle_policy` MUST be `dumb`. `metadata` causes fatal startup error.
- `rustore` / `apkpure`: `throttle_policy` may be `dumb` or `metadata`.
- Package names MUST be globally unique across all three store sources.

### GitHub Source (`github`)

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `token` | `string` | No | `null` | GitHub personal access token. Supports `$VARIABLE`. |
| `packages` | `list[object]` | Yes | `[]` | |

#### `packages` item fields (GitHub)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `repo` | `string` | Yes | Format: `owner/repo` |
| `mask` | `string` | Yes | Default file pattern for release asset matching (e.g., `*.apk`). Used when `arch_masks` is absent. |
| `arch_masks` | `object` | No | Map of architecture strings to file masks for per-architecture downloads. When present, the system downloads one asset per architecture listed in both `settings.architectures` and `arch_masks`. |

### WebDL Source (`webdl`)

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `packages` | `list[object]` | Yes | `[]` | |

#### `packages` item fields (WebDL)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `filename` | `string` | Yes | Default local filename for cache storage. SHOULD end in `.apk`. Used when `arch_files` is absent. |
| `url` | `string` | Yes | Default direct download URL. Used as stable identifier (hashed for `state.yaml`). Used when `arch_files` is absent. |
| `arch_files` | `object` | No | Map of architecture strings to objects with `filename` and `url` fields for per-architecture downloads. When present, the system downloads one file per architecture listed in both `settings.architectures` and `arch_files`. |

#### `arch_files` item fields (WebDL)

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `filename` | `string` | Yes | Local filename for this architecture's cache file. |
| `url` | `string` | Yes | Direct download URL for this architecture. |

## Variable Interpolation

Any string value in the configuration MAY contain `$VAR_NAME` syntax. At load time:

1. The system reads the raw YAML into a string map.
2. It recursively scans all string values for `$([A-Za-z_][A-Za-z0-9_]*)`.
3. Each match is replaced with `std::env::var(var_name)`.
4. If the environment variable is unset, the system prints a clear fatal error and exits with code `1`.

**Literal `$`**: To include a literal `$` in a value, use `$$`. The system replaces `$$` with `$` before env-var interpolation.

## Validation Rules

1. **Directory Existence**: If `cache_dir` or `output_dir` does not exist, auto-create and print a warning.
2. **Uniqueness**: Duplicate package names across `google_play`, `rustore`, `apkpure` → fatal error, exit `1`.
3. **PlayStore Policy**: `google_play` with `throttle_policy: metadata` → fatal error, exit `1`.
4. **Signing Warning**: `sign.enabled == true` with any missing signing sub-field → warning, continue.
5. **Repack/Sign Warning**: `repack_xapk == false` with `sign.enabled == true` → warning, continue.
6. **Empty Package List**: A source with an empty `packages` list is silently skipped.
