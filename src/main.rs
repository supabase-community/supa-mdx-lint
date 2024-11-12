use std::{env, io::BufWriter, path::PathBuf, process};

use anyhow::{Context, Result};
use clap::{ArgGroup, Parser};
use log::{debug, error};
use simplelog::{ColorChoice, Config as LogConfig, LevelFilter, TermLogger, TerminalMode};
use supa_mdx_lint::{Config, LintTarget, LinterBuilder, OutputFormatter, SimpleFormatter};

const DEFAULT_CONFIG_FILE: &str = "supa-mdx-lint.config.toml";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[clap(group(
            ArgGroup::new("verbosity")
                .args(&["debug", "silent"]),
        ))]
struct Args {
    /// File or directory to lint
    target: PathBuf,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long)]
    debug: bool,

    /// Do not write anything to the output
    #[arg(short, long)]
    silent: bool,
}

fn setup_logging(args: &Args) -> Result<LevelFilter> {
    let log_level: LevelFilter = match (args.silent, args.debug) {
        (true, false) => LevelFilter::Off,
        (false, true) => LevelFilter::Debug,
        _ => LevelFilter::Info,
    };
    TermLogger::init(
        log_level,
        LogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");

    Ok(log_level)
}

fn execute() -> Result<Result<()>> {
    let args = Args::parse();

    let log_level = setup_logging(&args)?;
    debug!("Log level set to {log_level}");

    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let target = current_dir.join(args.target);
    let target = LintTarget::FileOrDirectory(target);
    debug!("Lint target is {target:?}");

    let config_path = args.config.map_or_else(
        || current_dir.join(DEFAULT_CONFIG_FILE),
        |config| current_dir.join(config),
    );
    debug!("Config path is {config_path:?}");

    let linter = LinterBuilder::new()
        .configure(Config::from_config_file(config_path)?)
        .build()?;
    debug!("Linter built: {linter:?}");

    match linter.lint(target) {
        Ok(diagnostics) => {
            debug!("Linting completed successfully");

            if !args.silent {
                let formatter = SimpleFormatter;
                let stdout = std::io::stdout().lock();
                let mut stdout = BufWriter::new(stdout);
                formatter.format(&diagnostics, &mut stdout)?;
            }

            if diagnostics.iter().any(|d| !d.errors().is_empty()) {
                Ok(Err(anyhow::anyhow!("Linting errors found")))
            } else {
                Ok(Ok(()))
            }
        }
        Err(err) => {
            error!("Error: {err:?}");
            Err(err)
        }
    }
}

fn main() {
    match execute() {
        Ok(Ok(())) => process::exit(exitcode::OK),
        Ok(Err(_)) => process::exit(exitcode::DATAERR),
        // Not really, but we need to bubble better errors up to get a more
        // meaningful exit code.
        Err(_) => process::exit(exitcode::SOFTWARE),
    }
}
