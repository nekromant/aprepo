use aprepo::{config, download, lock, process, state, util};

use clap::{Parser, Subcommand};
use std::path::Path;
use util::logging;

#[derive(Parser)]
#[command(name = "aprepo")]
#[command(about = "APRepo APK Download and Processing Manager")]
#[command(version)]
struct Cli {
    #[arg(short, long, global = true, help = "Path to the YAML configuration file")]
    config: Option<String>,

    #[arg(short, long, global = true, help = "Emit debug-level details to stderr")]
    verbose: bool,

    #[arg(long, global = true, help = "Bypass throttle/mtime checks")]
    force: bool,

    #[arg(short, long, global = true, help = "Limit to a single Android package name")]
    package: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Bootstrap,
    Download,
    Process,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    logging::set_verbose(cli.verbose);

    // FR-020: Print full help when invoked with no arguments
    if cli.config.is_none() && cli.command.is_none() {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        cmd.print_help().unwrap();
        println!();
        std::process::exit(0);
    }

    let config_path = cli.config.as_ref().expect("--config is required");

    let exit_code = match &cli.command {
        Some(Commands::Bootstrap) => cmd_bootstrap(config_path),
        Some(Commands::Download) => {
            cmd_download(config_path, cli.force, cli.verbose, cli.package).await
        }
        Some(Commands::Process) => {
            cmd_process(config_path, cli.force, cli.verbose, cli.package)
        }
        None => {
            cmd_default(config_path, cli.force, cli.verbose, cli.package).await
        }
    };

    std::process::exit(exit_code);
}

fn cmd_bootstrap(config_path: &str) -> i32 {
    let path = Path::new(config_path);
    if path.exists() {
        logging::error(&format!("Configuration file already exists: {}", path.display()));
        return 1;
    }

    let template = r#"settings:
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
    packages: []
  rustore:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    packages: []
  apkpure:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    packages: []
  github:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    token: "$GITHUB_TOKEN"
    packages: []
  webdl:
    throttle_policy: metadata
    throttle_interval: 24h
    delay_between_requests: 2s
    packages: []
"#;

    if let Err(e) = std::fs::write(path, template) {
        logging::error(&format!("Cannot write config: {}", e));
        return 1;
    }

    let keystore_path = path.parent().unwrap_or(Path::new(".")).join("signing.p12");
    if let Err(e) = generate_keystore(&keystore_path) {
        logging::error(&format!("Cannot generate keystore: {}", e));
        return 1;
    }

    logging::info(&format!("Created config: {}", path.display()));
    logging::info(&format!("Created keystore: {}", keystore_path.display()));
    0
}

fn generate_keystore(path: &Path) -> Result<(), String> {
    use std::process::Command;
    let status = Command::new("keytool")
        .arg("-genkeypair")
        .arg("-v")
        .arg("-keystore")
        .arg(path)
        .arg("-alias")
        .arg("aprepo")
        .arg("-keyalg")
        .arg("RSA")
        .arg("-keysize")
        .arg("2048")
        .arg("-validity")
        .arg("10000")
        .arg("-storetype")
        .arg("PKCS12")
        .arg("-storepass")
        .arg("changeit")
        .arg("-keypass")
        .arg("changeit")
        .arg("-dname")
        .arg("CN=aprepo")
        .status()
        .map_err(|e| format!("keytool failed: {}", e))?;

    if !status.success() {
        return Err(format!("keytool exited with code {:?}", status.code()));
    }
    Ok(())
}

async fn cmd_download(config_path: &str, force: bool, verbose: bool, package_filter: Option<String>) -> i32 {
    let config = match config::Config::load(Path::new(config_path)) {
        Ok(c) => c,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let state_path = Path::new(&config.settings.cache_dir).join("state.yaml");
    let state = match state::State::load(&state_path) {
        Ok(s) => s,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let _lock = match lock::Lock::acquire(&state_path) {
        Ok(l) => l,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let mut orchestrator = download::DownloadOrchestrator::new(config, state, verbose, force, package_filter);
    match orchestrator.run().await {
        Ok(summary) => {
            logging::info(&format!(
                "Download: {} skipped, {} downloaded, {} failed",
                summary.skipped, summary.downloaded, summary.failed
            ));
            if summary.failed > 0 { 1 } else { 0 }
        }
        Err(e) => {
            logging::error(&e);
            1
        }
    }
}

fn cmd_process(config_path: &str, force: bool, verbose: bool, package_filter: Option<String>) -> i32 {
    let config = match config::Config::load(Path::new(config_path)) {
        Ok(c) => c,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let state_path = Path::new(&config.settings.cache_dir).join("state.yaml");
    let state = match state::State::load(&state_path) {
        Ok(s) => s,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let _lock = match lock::Lock::acquire(&state_path) {
        Ok(l) => l,
        Err(e) => {
            logging::error(&e);
            return 1;
        }
    };

    let orchestrator = process::ProcessOrchestrator::new(config, state, verbose, force, package_filter);
    match orchestrator.run() {
        Ok(summary) => {
            logging::info(&format!(
                "Process: {} skipped, {} processed, {} errors",
                summary.skipped, summary.processed, summary.errors
            ));
            if summary.errors > 0 { 1 } else { 0 }
        }
        Err(e) => {
            logging::error(&e);
            1
        }
    }
}

async fn cmd_default(config_path: &str, force: bool, verbose: bool, package_filter: Option<String>) -> i32 {
    let dl_code = cmd_download(config_path, force, verbose, package_filter.clone()).await;
    let proc_code = cmd_process(config_path, force, verbose, package_filter);
    if dl_code != 0 || proc_code != 0 {
        1
    } else {
        0
    }
}
