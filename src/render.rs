use anyhow::Result;
use clap::ValueEnum;

use crate::receipt::{CheckStatus, Receipt};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Markdown,
}

pub fn render_receipt(receipt: &Receipt, format: OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Text => Ok(render_text(receipt)),
        OutputFormat::Json => Ok(format!("{}\n", serde_json::to_string_pretty(receipt)?)),
        OutputFormat::Markdown => Ok(render_markdown(receipt)),
    }
}

fn render_text(receipt: &Receipt) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} ({})\n",
        receipt.tool.name, receipt.tool.version, receipt.mode
    ));
    out.push_str(&format!(
        "summary: {} passed, {} warned, {} failed, {} skipped\n",
        receipt.summary.passed,
        receipt.summary.warned,
        receipt.summary.failed,
        receipt.summary.skipped
    ));

    for check in &receipt.checks {
        out.push_str(&format!(
            "{} {:<7} {}",
            status_symbol(check.status),
            status_word(check.status),
            check.id
        ));
        if let Some(location) = &check.location {
            if location.source.is_some() || location.line.is_some() {
                out.push_str(" (");
                if let Some(source) = &location.source {
                    out.push_str(source);
                }
                if let Some(line) = location.line {
                    out.push_str(&format!(":{line}"));
                }
                out.push(')');
            }
        }
        out.push('\n');
        out.push_str(&format!("  {}\n", check.message));
    }

    if !receipt.commands.is_empty() {
        out.push_str("\ncommands:\n");
        for command in &receipt.commands {
            out.push_str(&format!(
                "  line {:<4} {:<11} {:<14} {}\n",
                command.line,
                syntax_word(command.syntax),
                command.name,
                command.outcome
            ));
        }
    }

    if !receipt.env_files.is_empty() {
        out.push_str("\nenvironment files:\n");
        for file in &receipt.env_files {
            out.push_str(&format!(
                "  {} {} records, {} bytes",
                file.kind.context_name(),
                file.records.len(),
                file.bytes
            ));
            if let Some(source) = &file.source {
                out.push_str(&format!(" ({source})"));
            }
            out.push('\n');
        }
    }

    out
}

fn render_markdown(receipt: &Receipt) -> String {
    let mut out = String::new();
    out.push_str("# GHA Command Proof\n\n");
    out.push_str(&format!(
        "- Tool: `{}` `{}`\n",
        receipt.tool.name, receipt.tool.version
    ));
    out.push_str(&format!("- Mode: `{}`\n", markdown_escape(&receipt.mode)));
    out.push_str(&format!("- Checked at: `{}`\n", receipt.checked_at));
    out.push_str(&format!(
        "- Summary: **{} passed**, **{} warned**, **{} failed**, **{} skipped**\n\n",
        receipt.summary.passed,
        receipt.summary.warned,
        receipt.summary.failed,
        receipt.summary.skipped
    ));

    out.push_str("| Status | Check | Location | Message |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for check in &receipt.checks {
        let location = check
            .location
            .as_ref()
            .map(|location| {
                let mut rendered = String::new();
                if let Some(source) = &location.source {
                    rendered.push_str(&markdown_escape(source));
                }
                if let Some(line) = location.line {
                    rendered.push_str(&format!(":{line}"));
                }
                rendered
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "| {} {} | `{}` | {} | {} |\n",
            status_symbol(check.status),
            status_word(check.status),
            markdown_escape(&check.id),
            location,
            markdown_escape(&check.message)
        ));
    }

    out
}

fn status_symbol(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "[PASS]",
        CheckStatus::Warn => "[WARN]",
        CheckStatus::Fail => "[FAIL]",
        CheckStatus::Skip => "[SKIP]",
    }
}

fn status_word(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "pass",
        CheckStatus::Warn => "warn",
        CheckStatus::Fail => "fail",
        CheckStatus::Skip => "skip",
    }
}

fn syntax_word(syntax: crate::CommandSyntax) -> &'static str {
    match syntax {
        crate::CommandSyntax::Modern => "modern",
        crate::CommandSyntax::Legacy => "legacy",
    }
}

fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}
