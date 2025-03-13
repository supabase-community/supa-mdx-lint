use std::{
    env,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
    time::Instant,
};

use anyhow::{Context, Result};
use bon::builder;
use clap::{error::ErrorKind, ArgGroup, CommandFactory, Parser};
#[cfg(feature = "interactive")]
use cli::InteractiveFixManager;
use glob::glob;
use log::{debug, error};
use simplelog::{ColorChoice, Config as LogConfig, LevelFilter, TermLogger, TerminalMode};
use supa_mdx_lint::{
    output::{internal::NativeOutputFormatter, LintOutput},
    Config, LintLevel, LintTarget, Linter,
};

mod cli;

const DEFAULT_CONFIG_FILE: &str = "supa-mdx-lint.config.toml";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[clap(group(
            ArgGroup::new("verbosity")
                .args(&["debug", "silent", "trace"]),
        ))]
struct Args {
    /// (Globs of) files or directories to lint
    target: Vec<String>,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Auto-fix any fixable errors
    #[arg(short, long)]
    fix: bool,

    #[cfg(feature = "interactive")]
    #[arg(short, long, requires_all = ["fix", "enable_experimental"], conflicts_with = "silent", hide = true)]
    interactive: bool,

    /// Output format
    #[arg(long, value_name = "FORMAT", default_value = "simple", value_parser = clap::value_parser!(NativeOutputFormatter), help = if cfg!(feature = "pretty") {r#"Output format - one of "simple", "markdown", "pretty", "rdf""#} else {r#"Output format - one of "simple", "markdown", "rdf""#})]
    format: NativeOutputFormatter,

    /// Turn debugging information on
    #[arg(short, long)]
    debug: bool,

    /// Do not write anything to the output
    #[arg(short, long)]
    silent: bool,

    #[cfg(debug_assertions)]
    #[arg(long)]
    trace: bool,

    #[arg(long, hide = true)]
    enable_experimental: bool,
}

fn setup_logging(args: &Args) -> Result<LevelFilter> {
    #[allow(unused_mut)]
    let mut log_level = if args.silent {
        LevelFilter::Off
    } else if args.debug {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    #[cfg(debug_assertions)]
    if args.trace {
        log_level = LevelFilter::Trace;
    }

    TermLogger::init(
        log_level,
        LogConfig::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");

    Ok(log_level)
}

#[builder]
fn get_targets<'targets>(
    targets: &'targets [String],
    linter: &Linter,
    #[builder(default = false)] expand_dirs: bool,
) -> Result<Vec<LintTarget<'targets>>> {
    let mut all_targets = Vec::new();

    for target in targets.iter() {
        let target = glob(target).context("Failed to parse glob pattern")?;
        target
            .into_iter()
            .filter_map(|res| res.ok())
            .filter(|path| linter.is_lintable(path))
            .map(LintTarget::FileOrDirectory)
            .for_each(|target| all_targets.push(target));
    }

    match expand_dirs {
        false => Ok(all_targets),
        true => {
            let mut new_targets = Vec::new();

            let mut idx = 0;
            while idx < all_targets.len() {
                let target = all_targets
                    .get(idx)
                    .expect("Just checked length of all_targets array");
                match target {
                    LintTarget::FileOrDirectory(path) if path.is_dir() => {
                        for entry in std::fs::read_dir(path).context("Failed to read directory")? {
                            let entry = entry.context("Failed to get directory entry")?;
                            let path = entry.path();
                            if linter.is_lintable(&path) {
                                all_targets.push(LintTarget::FileOrDirectory(path));
                            }
                        }

                        idx += 1;
                    }
                    LintTarget::FileOrDirectory(path) => {
                        new_targets.push(LintTarget::FileOrDirectory(path.clone()));
                        idx += 1;
                    }
                    _ => unreachable!(),
                }
            }

            Ok(new_targets)
        }
    }
}

fn get_diagnostics(targets: &[String], linter: &Linter) -> Result<Vec<LintOutput>> {
    let all_targets = get_targets().targets(targets).linter(linter).call()?;
    debug!("Lint targets: {targets:#?}");

    let mut diagnostics = Vec::new();
    for target in all_targets {
        match linter.lint(&target) {
            Ok(mut result) => {
                debug!("Successfully linted {target:?}");
                diagnostics.append(&mut result);
            }
            Err(err) => {
                error!("Error linting {target:?}: {err:#?}");
                return Err(err);
            }
        }
    }
    Ok(diagnostics)
}

fn execute(args: Args) -> Result<Result<()>> {
    let start = Instant::now();

    let log_level = setup_logging(&args)?;
    debug!("Log level set to {log_level}");

    if args.target.is_empty() {
        let mut cmd = Args::command();
        cmd.error(
            ErrorKind::MissingRequiredArgument,
            "The following required arguments were not provided:\n    [TARGET]",
        )
        .exit();
    };

    let current_dir = env::current_dir().context("Failed to get current directory")?;
    let config_path = args.config.map_or_else(
        || current_dir.join(DEFAULT_CONFIG_FILE),
        |config| current_dir.join(config),
    );
    debug!("Config path is {config_path:?}");

    let config = Config::from_config_file(config_path)?;
    let linter = Linter::builder().config(config).build()?;
    debug!("Linter built: {linter:#?}");

    let stdout = std::io::stdout().lock();
    let mut stdout = BufWriter::new(stdout);

    #[cfg(feature = "interactive")]
    if args.interactive {
        return Ok(InteractiveFixManager::new(
            &linter,
            get_targets()
                .targets(&args.target)
                .expand_dirs(true)
                .linter(&linter)
                .call()?,
        )
        .run());
    }

    let mut diagnostics = get_diagnostics(&args.target, &linter)?;

    #[allow(unused_mut)]
    let mut fix_only = args.fix;
    #[cfg(feature = "interactive")]
    if args.interactive {
        fix_only = false;
    }

    if fix_only {
        let (num_files_fixed, num_errors_fixed) = linter.fix(&diagnostics)?;
        if !args.silent {
            writeln!(
                stdout,
                "Fixed {num_errors_fixed} error{} in {num_files_fixed} file{}",
                if num_errors_fixed != 1 { "s" } else { "" },
                if num_files_fixed != 1 { "s" } else { "" },
            )?;
            writeln!(stdout, "Checking for oustanding errors...")?;
            writeln!(stdout)?;
        }
        diagnostics = get_diagnostics(&args.target, &linter)?;
    }

    if !args.silent {
        let output = args.format.format(&diagnostics)?;
        write!(stdout, "{}", output)?;
        if args.format.should_log_metadata() {
            let millis = start.elapsed().as_millis();
            if millis < 1000 {
                writeln!(stdout, "🕚 Done in {:.1} seconds", millis as f64 / 1000.0)?;
            } else {
                let seconds = millis / 1000;
                writeln!(
                    stdout,
                    "🕚 Done in {} second{}",
                    seconds,
                    if seconds == 1 { "" } else { "s" }
                )?;
            }
        }
    }

    stdout.flush()?;

    if diagnostics
        .iter()
        .any(|d| d.errors().iter().any(|e| e.level() == LintLevel::Error))
    {
        Ok(Err(anyhow::anyhow!("Linting errors found")))
    } else {
        Ok(Ok(()))
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    let silent = args.silent;

    match execute(args) {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(_)) => ExitCode::from(TryInto::<u8>::try_into(exitcode::DATAERR).unwrap()),
        // Not really, but we need to bubble better errors up to get a more
        // meaningful exit code.
        Err(err) => {
            if !silent {
                eprintln!("{err}");
            }
            ExitCode::from(TryInto::<u8>::try_into(exitcode::SOFTWARE).unwrap())
        }
    }
}
