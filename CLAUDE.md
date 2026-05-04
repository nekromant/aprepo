# v2 Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-05-04

## Active Technologies
- Filesystem only — cache directory (raw downloads + `state.yaml`), output directory (processed APKs) (001-aprepo-apk-manager)

- Rust 1.78+ (edition 2021) + `clap` (CLI), `serde`+`serde_yaml` (config), `zip` (archive validation), `quick-xml` (manifest extraction), `fs2` (file locking), `reqwest` (WebDL/HEAD requests), `tokio` (async runtime for GitHub API calls), `tempfile` (temp dirs) (001-aprepo-apk-manager)

## Project Structure

```text
src/
tests/
```

## Commands

cargo test [ONLY COMMANDS FOR ACTIVE TECHNOLOGIES][ONLY COMMANDS FOR ACTIVE TECHNOLOGIES] cargo clippy

## Code Style

Rust 1.78+ (edition 2021): Follow standard conventions

## Recent Changes
- 001-aprepo-apk-manager: Added Rust 1.78+ (edition 2021) + `clap` (CLI), `serde`+`serde_yaml` (config), `zip` (archive validation), `quick-xml` (manifest extraction), `fs2` (file locking), `reqwest` (WebDL/HEAD requests), `tokio` (async runtime for GitHub API calls), `tempfile` (temp dirs)

- 001-aprepo-apk-manager: Added Rust 1.78+ (edition 2021) + `clap` (CLI), `serde`+`serde_yaml` (config), `zip` (archive validation), `quick-xml` (manifest extraction), `fs2` (file locking), `reqwest` (WebDL/HEAD requests), `tokio` (async runtime for GitHub API calls), `tempfile` (temp dirs)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
