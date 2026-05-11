//! GitHub Actions workflow command and environment-file verifier.
//!
//! `gha-command-proof` parses the command channel that actions write to
//! stdout/stderr, plus the file command protocol exposed through
//! `GITHUB_ENV`, `GITHUB_OUTPUT`, `GITHUB_STATE`, `GITHUB_PATH`, and
//! `GITHUB_STEP_SUMMARY`.

mod command;
mod envfile;
mod receipt;
mod render;

use std::collections::BTreeMap;

use chrono::Utc;

pub use command::{
    CommandRecord, CommandSyntax, LogAnalysis, ParsedCommand, analyze_command_stream, escape_data,
    escape_legacy, escape_property, parse_command_line, unescape_data, unescape_legacy,
    unescape_property,
};
pub use envfile::{EnvFileAnalysis, EnvFileKind, EnvFileRecord, analyze_env_file};
pub use receipt::{Check, CheckStatus, Location, Receipt, Summary, ToolReceipt};
pub use render::{OutputFormat, render_receipt};

/// Options shared by command-stream and environment-file analysis.
#[derive(Clone, Debug, Default)]
pub struct ProofOptions {
    /// Treat warnings as failed checks in the rendered summary and process exit.
    pub strict: bool,
}

/// Inputs for a single GitHub Actions step proof.
#[derive(Clone, Debug, Default)]
pub struct StepInput {
    /// Optional stdout/stderr command stream.
    pub log: Option<NamedText>,
    /// Optional `GITHUB_ENV` file contents.
    pub github_env: Option<NamedText>,
    /// Optional `GITHUB_OUTPUT` file contents.
    pub github_output: Option<NamedText>,
    /// Optional `GITHUB_STATE` file contents.
    pub github_state: Option<NamedText>,
    /// Optional `GITHUB_PATH` file contents.
    pub github_path: Option<NamedText>,
    /// Optional `GITHUB_STEP_SUMMARY` file contents.
    pub github_step_summary: Option<NamedText>,
}

/// Text with a stable source label used in receipts.
#[derive(Clone, Debug)]
pub struct NamedText {
    pub source: String,
    pub text: String,
}

impl NamedText {
    pub fn new(source: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            text: text.into(),
        }
    }
}

/// Analyze one command stream.
pub fn prove_log(input: NamedText, options: &ProofOptions) -> Receipt {
    let analysis = analyze_command_stream(&input.text, Some(input.source));
    receipt_from_parts(
        "log",
        options,
        analysis.checks,
        analysis.commands,
        Vec::new(),
    )
}

/// Analyze one environment file.
pub fn prove_env_file(kind: EnvFileKind, input: NamedText, options: &ProofOptions) -> Receipt {
    let analysis = analyze_env_file(kind, &input.text, Some(input.source), &[]);
    receipt_from_parts(
        "env-file",
        options,
        analysis.checks.clone(),
        Vec::new(),
        vec![analysis],
    )
}

/// Analyze a whole step boundary: command stream first, then all supplied file commands.
pub fn prove_step(input: StepInput, options: &ProofOptions) -> Receipt {
    let mut checks = Vec::new();
    let mut commands = Vec::new();
    let mut files = Vec::new();
    let mut masks = Vec::new();

    if let Some(log) = input.log {
        let analysis = analyze_command_stream(&log.text, Some(log.source));
        masks = analysis.mask_values;
        checks.extend(analysis.checks);
        commands.extend(analysis.commands);
    } else {
        checks.push(Check::skip(
            "step.log",
            "no command stream supplied for this step",
            None,
        ));
    }

    for (kind, text) in [
        (EnvFileKind::Env, input.github_env),
        (EnvFileKind::Output, input.github_output),
        (EnvFileKind::State, input.github_state),
        (EnvFileKind::Path, input.github_path),
        (EnvFileKind::StepSummary, input.github_step_summary),
    ] {
        if let Some(text) = text {
            let analysis = analyze_env_file(kind, &text.text, Some(text.source), &masks);
            checks.extend(analysis.checks.clone());
            files.push(analysis);
        }
    }

    if files.is_empty() {
        checks.push(Check::skip(
            "step.env_files",
            "no environment files supplied for this step",
            None,
        ));
    }

    receipt_from_parts("step", options, checks, commands, files)
}

fn receipt_from_parts(
    mode: impl Into<String>,
    options: &ProofOptions,
    checks: Vec<Check>,
    commands: Vec<CommandRecord>,
    env_files: Vec<EnvFileAnalysis>,
) -> Receipt {
    let mut summary = Summary::from_checks(&checks);
    if options.strict && summary.warned > 0 {
        summary.failed += summary.warned;
        summary.warned = 0;
    }

    Receipt {
        schema_version: 1,
        tool: ToolReceipt {
            name: "gha-command-proof",
            version: env!("CARGO_PKG_VERSION"),
        },
        checked_at: Utc::now(),
        mode: mode.into(),
        summary,
        checks,
        commands,
        env_files,
        metadata: BTreeMap::new(),
    }
}
