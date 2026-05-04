# Data Model: APRepo APK Download and Processing Manager

## Entities

### Config

The fully-parsed, validated configuration loaded at startup.

| Field | Type | Constraints |
|-------|------|-------------|
| `settings` | `Settings` | Required |
| `sources` | `HashMap<String, Source>` | Keys: `google_play`, `rustore`, `apkpure`, `github`, `webdl` (all optional) |

**Validation Rules**:
- `settings.cache_dir` and `settings.output_dir` must be non-empty strings.
- Duplicate package names across store sources (`google_play`, `rustore`, `apkpure`) are a fatal error at startup.
- `google_play` sources MUST have `throttle_policy: dumb`; `metadata` is a fatal error.
- `settings.sign.enabled == true` with missing signing fields produces a warning but does not abort.
- `settings.repack_xapk == false` with `settings.sign.enabled == true` produces a warning (signing has no effect).

### Settings

| Field | Type | Default | Constraints |
|-------|------|---------|-------------|
| `cache_dir` | `String` | — | Required; auto-created if missing |
| `output_dir` | `String` | — | Required; auto-created if missing |
| `retention_depth` | `u32` | `1` | Number of previous versions to keep |
| `architectures` | `Vec<String>` | `["arm64-v8a"]` | Supported: `arm64-v8a`, `armeabi-v7a`, `armeabi`, `x86_64`, `x86` |
| `density` | `String` | `"xxhdpi"` | Supported: `xxxhdpi`, `xxhdpi`, `xhdpi`, `hdpi`, `mdpi`, `ldpi`, `nodpi`, `tvdpi` |
| `repack_xapk` | `bool` | `false` | When `false`, XAPK files are copied unchanged |
| `sign` | `Option<SignSettings>` | `None` | Optional signing configuration |

### SignSettings

| Field | Type | Constraints |
|-------|------|-------------|
| `enabled` | `bool` | Default `false` |
| `keystore_file` | `String` | Required if `enabled` |
| `keystore_password` | `String` | Required if `enabled`; supports `$VARIABLE` |
| `key_alias` | `String` | Required if `enabled` |
| `key_password` | `String` | Required if `enabled`; supports `$VARIABLE` |

### Source (Store)

Applies to `google_play`, `rustore`, `apkpure`.

| Field | Type | Default | Constraints |
|-------|------|---------|-------------|
| `throttle_policy` | `String` | `"dumb"` (PlayStore), `"metadata"` (others) | `dumb` or `metadata` |
| `throttle_interval` | `Duration` | `24h` | Parsed from `<number>h` or `<number>d` |
| `delay_between_requests` | `Duration` | `2s` | Parsed from `<number>s` or `<number>m` |
| `token` | `Option<String>` | `None` | Supports `$VARIABLE` |
| `packages` | `Vec<String>` | `[]` | Package names; globally unique across all store sources |

### Source (GitHub)

| Field | Type | Default | Constraints |
|-------|------|---------|-------------|
| `throttle_policy` | `String` | `"metadata"` | `dumb` or `metadata` |
| `throttle_interval` | `Duration` | `24h` | |
| `delay_between_requests` | `Duration` | `2s` | |
| `token` | `Option<String>` | `None` | Supports `$VARIABLE` |
| `packages` | `Vec<GitHubPackage>` | `[]` | |

### GitHubPackage

| Field | Type | Constraints |
|-------|------|-------------|
| `repo` | `String` | Format: `owner/repo` |
| `mask` | `String` | File glob for release asset matching |
| `arch_masks` | `Option<HashMap<String, String>>` | Map of arch → mask for per-architecture downloads |

### Source (WebDL)

| Field | Type | Default | Constraints |
|-------|------|---------|-------------|
| `throttle_policy` | `String` | `"metadata"` | `dumb` or `metadata` |
| `throttle_interval` | `Duration` | `24h` | |
| `delay_between_requests` | `Duration` | `2s` | |
| `packages` | `Vec<WebDlPackage>` | `[]` | |

### WebDlPackage

| Field | Type | Constraints |
|-------|------|-------------|
| `filename` | `String` | Local cache filename; SHOULD end in `.apk` |
| `url` | `String` | Direct download URL; used as stable identifier (hashed for `state.yaml`) |
| `arch_files` | `Option<HashMap<String, ArchFile>>` | Map of arch → `{filename, url}` for per-architecture downloads |

### ArchFile

| Field | Type | Constraints |
|-------|------|-------------|
| `filename` | `String` | Local cache filename for this architecture |
| `url` | `String` | Direct download URL for this architecture |

### State

Persisted in `cache_dir/state.yaml`.

| Field | Type | Constraints |
|-------|------|-------------|
| `versions` | `HashMap<String, VersionRecord>` | Key format: `<source>:<identifier>` |
| `capabilities` | `HashMap<String, SourceCapability>` | Key: source name (e.g., `google_play`) |

### SourceCapability

Tracks runtime-discovered capabilities of a download source.

| Field | Type | Description |
|-------|------|-------------|
| `per_arch_downloads` | `Option<bool>` | `Some(true)` = source supports per-arch downloads; `Some(false)` = unsupported; `None` = not yet tested |

### VersionRecord

| Field | Type | Description |
|-------|------|-------------|
| `versions` | `HashMap<String, String>` | Architecture → version (semantic version, release tag, or header hash). Key `"universal"` is used for non-architecture-specific downloads. |
| `cached_files` | `HashMap<String, String>` | Architecture → relative path within cache_dir. Key `"universal"` is used for non-architecture-specific downloads.

**Key formats**:
- Store source: `<source>:<package_name>` (e.g., `google_play:com.example.app`)
- GitHub: `github:<stable_hash(repo+mask)>`
- WebDL (universal): `webdl:<stable_hash(url)>`
- WebDL (per-arch): `webdl:<stable_hash(sorted_arch_urls)>`

### Package (Runtime)

Internal union type representing a package after config parsing.

| Variant | Fields |
|---------|--------|
| `Store` | `source: String`, `name: String` |
| `GitHub` | `source: String`, `repo: String`, `mask: String`, `hash: String` |
| `WebDL` | `source: String`, `filename: String`, `url: String`, `hash: String` |

### DownloadResult

| Variant | Meaning |
|---------|---------|
| `Skipped` | Throttled or unchanged version |
| `Downloaded(String)` | Path to new cache file |
| `Failed(String)` | Error message; triggers one retry |

### ProcessResult

| Variant | Meaning |
|---------|---------|
| `Skipped` | Output mtime >= cache mtime |
| `Processed(String)` | Path to output APK |
| `Error(String)` | Validation or merge failure |

## State Transitions

### Download Lifecycle

```
[Config Loaded]
   |
   v
[Validate Uniqueness] --duplicate--> [FATAL ERROR, exit 1]
   |
   v
[Acquire Lock on state.yaml] --locked--> [ERROR, exit 1]
   |
   v
[Read state.yaml]
   |
   v
[For each package] --all skipped--> [Done]
   |
   v
[Check Throttle] --not expired--> [Skip]
   |
   v
[Check Version Metadata] --unchanged--> [Skip]
   |
   v
[Download] --success--> [Validate ZIP] --valid--> [Update Cache + state.yaml]
   |                  --invalid--> [Delete, Retry once]
   |                  --fail--> [Retry once]
   |                  --retry fail--> [Preserve old cache]
   v
[Done] --> [Release Lock]
```

### Process Lifecycle

```
[Config Loaded]
   |
   v
[Scan cache_dir]
   |
   v
[For each cache file] --mtime >= output mtime--> [Skip]
   |                  --corrupt--> [Log error, Skip]
   |                  --XAPK + repack_xapk=false--> [Copy unchanged]
   |                  --XAPK + repack_xapk=true--> [Repack per arch]
   |                  --APK--> [Copy unchanged]
   |
   v
[Apply Retention] --> [Purge old versions + removed packages]
   |
   v
[Done]
```

### XAPK Repack Lifecycle (when `repack_xapk: true`)

```
[Read XAPK as ZIP]
   |
   v
[Parse manifest.json] --missing/invalid--> [ERROR, skip file]
   |
   v
[Extract base APK + splits]
   |
   v
[For each target architecture]
   |   |
   |   v
   | [Find arch split] --missing--> [Fallback next arch] --all missing--> [Skip arch, warning]
   |   |
   |   v
   | [Merge base + arch split + all density splits]
   |   |
   |   v
   | [apktool decode → merge → rebuild]
   |   |
   |   v
   | [zipalign]
   |   |
   |   v
   | [apksigner sign] --missing/invalid sign config--> [Warning, skip signing]
   |   |
   |   v
   | [Output APK]
   |
   v
[Cleanup temp dirs]
```
