use std::{env, path::PathBuf};

use anyhow::Result;
use clap::Parser;
use log::debug;
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};

const DEFAULT_CONFIG_FILE: &str = "supa-mdx-lint.config.toml";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// File or directory to lint
    target: PathBuf,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long)]
    debug: bool,
}

fn setup_logging(debug: bool) -> Result<LevelFilter> {
    let log_level: LevelFilter = match debug {
        true => LevelFilter::Debug,
        false => LevelFilter::Info,
    };
    TermLogger::init(
        log_level,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");

    Ok(log_level)
}

fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = setup_logging(args.debug)?;
    debug!("Log level set to {log_level}");

    let current_dir = env::current_dir().expect("Failed to get current directory");

    let target = current_dir.join(args.target);
    debug!("Lint target is {target:?}");

    let config_path = args.config.map_or_else(
        || current_dir.join(DEFAULT_CONFIG_FILE),
        |config| current_dir.join(config),
    );
    debug!("Config path is {config_path:?}");

    Ok(())
}
