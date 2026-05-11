use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};
use gha_command_proof::{
    EnvFileKind, NamedText, OutputFormat, ProofOptions, StepInput, prove_env_file, prove_log,
    prove_step, render_receipt,
};

#[derive(Parser, Debug)]
#[command(name = "gha-command-proof")]
#[command(about = "Verifier for GitHub Actions workflow commands and environment files")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate a stdout/stderr workflow command stream.
    Log(LogArgs),
    /// Validate one GitHub Actions environment file.
    EnvFile(EnvFileArgs),
    /// Validate a step's command stream plus any generated environment files.
    Step(StepArgs),
}

#[derive(Args, Debug)]
struct CommonArgs {
    /// Output format.
    #[arg(long, default_value = "text")]
    format: OutputFormat,

    /// Write the receipt to this path instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Treat warnings as failures.
    #[arg(long)]
    strict: bool,
}

#[derive(Args, Debug)]
struct LogArgs {
    /// Log file to validate. Use `-` or omit the path to read stdin.
    path: Option<PathBuf>,

    /// Write the redacted log stream to this path.
    #[arg(long)]
    redacted_log_output: Option<PathBuf>,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Args, Debug)]
struct EnvFileArgs {
    /// Environment file kind.
    #[arg(long)]
    kind: EnvFileKind,

    /// File to validate. Use `-` to read stdin.
    path: PathBuf,

    #[command(flatten)]
    common: CommonArgs,
}

#[derive(Args, Debug)]
struct StepArgs {
    /// Optional stdout/stderr command stream.
    #[arg(long)]
    log: Option<PathBuf>,

    /// Optional GITHUB_ENV file.
    #[arg(long = "github-env")]
    github_env: Option<PathBuf>,

    /// Optional GITHUB_OUTPUT file.
    #[arg(long = "github-output")]
    github_output: Option<PathBuf>,

    /// Optional GITHUB_STATE file.
    #[arg(long = "github-state")]
    github_state: Option<PathBuf>,

    /// Optional GITHUB_PATH file.
    #[arg(long = "github-path")]
    github_path: Option<PathBuf>,

    /// Optional GITHUB_STEP_SUMMARY file.
    #[arg(long = "github-step-summary")]
    github_step_summary: Option<PathBuf>,

    /// Write the redacted log stream to this path.
    #[arg(long)]
    redacted_log_output: Option<PathBuf>,

    #[command(flatten)]
    common: CommonArgs,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Log(args) => run_log(args),
        Command::EnvFile(args) => run_env_file(args),
        Command::Step(args) => run_step(args),
    }
}

fn run_log(args: LogArgs) -> Result<()> {
    let input = read_named_text(args.path.as_ref(), "stdin")?;
    let analysis =
        gha_command_proof::analyze_command_stream(&input.text, Some(input.source.clone()));
    if let Some(path) = args.redacted_log_output {
        fs::write(&path, &analysis.redacted_log)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    let receipt = prove_log(input, &options(&args.common));
    finish(receipt, args.common)
}

fn run_env_file(args: EnvFileArgs) -> Result<()> {
    let input = read_named_text(Some(&args.path), args.kind.context_name())?;
    let receipt = prove_env_file(args.kind, input, &options(&args.common));
    finish(receipt, args.common)
}

fn run_step(args: StepArgs) -> Result<()> {
    let mut step = StepInput::default();
    if let Some(path) = args.log.as_ref() {
        step.log = Some(read_named_text(Some(path), "log")?);
    }
    if let Some(path) = args.github_env.as_ref() {
        step.github_env = Some(read_named_text(Some(path), "GITHUB_ENV")?);
    }
    if let Some(path) = args.github_output.as_ref() {
        step.github_output = Some(read_named_text(Some(path), "GITHUB_OUTPUT")?);
    }
    if let Some(path) = args.github_state.as_ref() {
        step.github_state = Some(read_named_text(Some(path), "GITHUB_STATE")?);
    }
    if let Some(path) = args.github_path.as_ref() {
        step.github_path = Some(read_named_text(Some(path), "GITHUB_PATH")?);
    }
    if let Some(path) = args.github_step_summary.as_ref() {
        step.github_step_summary = Some(read_named_text(Some(path), "GITHUB_STEP_SUMMARY")?);
    }

    if let (Some(log), Some(path)) = (step.log.as_ref(), args.redacted_log_output.as_ref()) {
        let analysis =
            gha_command_proof::analyze_command_stream(&log.text, Some(log.source.clone()));
        fs::write(path, &analysis.redacted_log)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    let receipt = prove_step(step, &options(&args.common));
    finish(receipt, args.common)
}

fn finish(receipt: gha_command_proof::Receipt, common: CommonArgs) -> Result<()> {
    let failed = receipt.summary.failed > 0;
    let rendered = render_receipt(&receipt, common.format)?;
    if let Some(path) = common.output {
        fs::write(&path, rendered)
            .with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        print!("{rendered}");
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

fn options(common: &CommonArgs) -> ProofOptions {
    ProofOptions {
        strict: common.strict,
    }
}

fn read_named_text(path: Option<&PathBuf>, stdin_label: &str) -> Result<NamedText> {
    match path {
        Some(path) if path.as_os_str() == "-" => read_stdin(stdin_label),
        Some(path) => {
            let text = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let source = normalize_display_path(path)?;
            Ok(NamedText::new(source, text))
        }
        None => read_stdin(stdin_label),
    }
}

fn read_stdin(label: &str) -> Result<NamedText> {
    let mut text = String::new();
    io::stdin()
        .read_to_string(&mut text)
        .context("failed to read stdin")?;
    Ok(NamedText::new(label, text))
}

fn normalize_display_path(path: &Path) -> Result<String> {
    let normalized = if path.exists() {
        dunce::canonicalize(path)
            .with_context(|| format!("failed to resolve {}", path.display()))?
    } else {
        path.to_path_buf()
    };
    Ok(Utf8PathBuf::from_path_buf(normalized)
        .map(|path| path.to_string())
        .unwrap_or_else(|path| path.display().to_string()))
}
