use serde::Serialize;

use crate::command::{masks_contain_exact, redact_with_masks};
use crate::receipt::{Check, Location};

const STEP_SUMMARY_LIMIT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct EnvFileAnalysis {
    pub kind: EnvFileKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub bytes: usize,
    pub records: Vec<EnvFileRecord>,
    #[serde(skip)]
    pub checks: Vec<Check>,
}

impl EnvFileAnalysis {
    fn with_checks(mut self, checks: Vec<Check>) -> Self {
        self.checks = checks;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum EnvFileKind {
    Env,
    Output,
    State,
    Path,
    StepSummary,
}

impl EnvFileKind {
    pub fn context_name(self) -> &'static str {
        match self {
            Self::Env => "GITHUB_ENV",
            Self::Output => "GITHUB_OUTPUT",
            Self::State => "GITHUB_STATE",
            Self::Path => "GITHUB_PATH",
            Self::StepSummary => "GITHUB_STEP_SUMMARY",
        }
    }

    fn check_prefix(self) -> &'static str {
        match self {
            Self::Env => "env_file.env",
            Self::Output => "env_file.output",
            Self::State => "env_file.state",
            Self::Path => "env_file.path",
            Self::StepSummary => "env_file.step_summary",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EnvFileRecord {
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub kind: EnvFileRecordKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub value: String,
    pub value_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnvFileRecordKind {
    Assignment,
    Heredoc,
    Path,
    Summary,
}

pub fn analyze_env_file(
    kind: EnvFileKind,
    text: &str,
    source: Option<String>,
    masks: &[String],
) -> EnvFileAnalysis {
    analyze_env_file_with_checks(kind, text, source, masks)
}

pub(crate) fn analyze_env_file_with_checks(
    kind: EnvFileKind,
    text: &str,
    source: Option<String>,
    masks: &[String],
) -> EnvFileAnalysis {
    match kind {
        EnvFileKind::Path => analyze_path_file(kind, text, source, masks),
        EnvFileKind::StepSummary => analyze_step_summary(kind, text, source, masks),
        EnvFileKind::Env | EnvFileKind::Output | EnvFileKind::State => {
            analyze_key_value_file(kind, text, source, masks)
        }
    }
}

fn analyze_key_value_file(
    kind: EnvFileKind,
    text: &str,
    source: Option<String>,
    masks: &[String],
) -> EnvFileAnalysis {
    let mut checks = Vec::new();
    let mut records = Vec::new();
    let lines = logical_lines(text);
    let mut index = 0;

    while index < lines.len() {
        let line_number = lines[index].number;
        let line = lines[index].text.as_str();
        index += 1;

        if line.is_empty() {
            continue;
        }

        let equals = line.find('=');
        let heredoc = line.find("<<");
        if let Some(equals) = equals
            && heredoc.is_none_or(|heredoc| equals < heredoc)
        {
            let name = &line[..equals];
            let value = &line[equals + 1..];
            if name.is_empty() {
                checks.push(Check::fail(
                    format!("{}.name", kind.check_prefix()),
                    "environment file assignment name must not be empty",
                    Some(Location::line(source.as_deref(), line_number)),
                ));
                continue;
            }
            validate_key_value(
                kind,
                name,
                value,
                line_number,
                source.as_deref(),
                masks,
                &mut checks,
            );
            records.push(record(
                line_number,
                None,
                EnvFileRecordKind::Assignment,
                Some(name),
                value,
                masks,
            ));
            continue;
        }

        if let Some(heredoc) = heredoc
            && equals.is_none_or(|equals| heredoc < equals)
        {
            let name = &line[..heredoc];
            let delimiter = &line[heredoc + 2..];
            if name.is_empty() || delimiter.is_empty() {
                checks.push(Check::fail(
                    format!("{}.heredoc.header", kind.check_prefix()),
                    "heredoc syntax requires non-empty name and delimiter",
                    Some(Location::line(source.as_deref(), line_number)),
                ));
                continue;
            }

            let value_start = index;
            let mut value_end = None;
            while index < lines.len() {
                if lines[index].text == delimiter {
                    value_end = Some(index);
                    break;
                }
                index += 1;
            }

            let Some(end_index) = value_end else {
                checks.push(Check::fail(
                    format!("{}.heredoc.delimiter", kind.check_prefix()),
                    "matching heredoc delimiter was not found",
                    Some(Location::line(source.as_deref(), line_number)),
                ));
                break;
            };

            let value = lines[value_start..end_index]
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            validate_key_value(
                kind,
                name,
                &value,
                line_number,
                source.as_deref(),
                masks,
                &mut checks,
            );
            records.push(record(
                line_number,
                Some(lines[end_index].number),
                EnvFileRecordKind::Heredoc,
                Some(name),
                &value,
                masks,
            ));
            index = end_index + 1;
            continue;
        }

        checks.push(Check::fail(
            format!("{}.format", kind.check_prefix()),
            "environment file line must use `NAME=VALUE` or `NAME<<DELIMITER` syntax",
            Some(Location::line(source.as_deref(), line_number)),
        ));
    }

    if records.is_empty() {
        checks.push(Check::skip(
            format!("{}.records", kind.check_prefix()),
            format!("{} contains no records", kind.context_name()),
            source
                .as_deref()
                .map(|source| Location::new(Some(source.to_string()), None)),
        ));
    } else {
        checks.push(Check::pass(
            format!("{}.records", kind.check_prefix()),
            format!("parsed {} {} records", records.len(), kind.context_name()),
            source
                .as_deref()
                .map(|source| Location::new(Some(source.to_string()), None)),
        ));
    }

    EnvFileAnalysis {
        kind,
        source,
        bytes: text.len(),
        records,
        checks: Vec::new(),
    }
    .with_checks(checks)
}

fn analyze_path_file(
    kind: EnvFileKind,
    text: &str,
    source: Option<String>,
    masks: &[String],
) -> EnvFileAnalysis {
    let mut records = Vec::new();
    for line in logical_lines(text) {
        if line.text.is_empty() {
            continue;
        }
        records.push(record(
            line.number,
            None,
            EnvFileRecordKind::Path,
            None,
            &line.text,
            masks,
        ));
    }

    let location = source
        .as_deref()
        .map(|source| Location::new(Some(source.to_string()), None));
    let checks = if records.is_empty() {
        vec![Check::skip(
            "env_file.path.records",
            "GITHUB_PATH contains no path records",
            location,
        )]
    } else {
        vec![Check::pass(
            "env_file.path.records",
            format!("parsed {} GITHUB_PATH records", records.len()),
            location,
        )]
    };

    EnvFileAnalysis {
        kind,
        source,
        bytes: text.len(),
        records,
        checks: Vec::new(),
    }
    .with_checks(checks)
}

fn analyze_step_summary(
    kind: EnvFileKind,
    text: &str,
    source: Option<String>,
    masks: &[String],
) -> EnvFileAnalysis {
    let mut checks = Vec::new();
    let location = source
        .as_deref()
        .map(|source| Location::new(Some(source.to_string()), None));
    if text.is_empty() {
        checks.push(Check::skip(
            "env_file.step_summary.content",
            "GITHUB_STEP_SUMMARY is empty",
            location.clone(),
        ));
    } else {
        checks.push(Check::pass(
            "env_file.step_summary.content",
            "GITHUB_STEP_SUMMARY contains Markdown content",
            location.clone(),
        ));
    }

    if text.len() > STEP_SUMMARY_LIMIT_BYTES {
        checks.push(Check::fail(
            "env_file.step_summary.size",
            format!(
                "GITHUB_STEP_SUMMARY is {} bytes, above the 1 MiB runner attachment limit",
                text.len()
            ),
            location,
        ));
    } else {
        checks.push(Check::pass(
            "env_file.step_summary.size",
            "GITHUB_STEP_SUMMARY is within the 1 MiB runner attachment limit",
            location,
        ));
    }

    let records = if text.is_empty() {
        Vec::new()
    } else {
        vec![record(
            1,
            Some(logical_lines(text).last().map_or(1, |line| line.number)),
            EnvFileRecordKind::Summary,
            None,
            text,
            masks,
        )]
    };

    EnvFileAnalysis {
        kind,
        source,
        bytes: text.len(),
        records,
        checks: Vec::new(),
    }
    .with_checks(checks)
}

fn validate_key_value(
    kind: EnvFileKind,
    name: &str,
    value: &str,
    line: usize,
    source: Option<&str>,
    masks: &[String],
    checks: &mut Vec<Check>,
) {
    let location = Some(Location::line(source, line));
    match kind {
        EnvFileKind::Env => {
            if name.eq_ignore_ascii_case("NODE_OPTIONS") {
                checks.push(Check::fail(
                    "env_file.env.node_options",
                    "GITHUB_ENV cannot set NODE_OPTIONS on GitHub runners",
                    location.clone(),
                ));
            }
            if is_default_runner_variable(name) {
                checks.push(Check::warn(
                    "env_file.env.default_variable",
                    format!(
                        "`{name}` is a default runner variable and cannot be reliably overwritten"
                    ),
                    location,
                ));
            }
        }
        EnvFileKind::Output => {
            if masks_contain_exact(masks, value) {
                checks.push(Check::fail(
                    "env_file.output.masked_value",
                    "GITHUB_OUTPUT attempts to set a value previously registered with add-mask",
                    location,
                ));
            }
        }
        EnvFileKind::State | EnvFileKind::Path | EnvFileKind::StepSummary => {}
    }
}

fn record(
    line: usize,
    end_line: Option<usize>,
    kind: EnvFileRecordKind,
    name: Option<&str>,
    value: &str,
    masks: &[String],
) -> EnvFileRecord {
    EnvFileRecord {
        line,
        end_line,
        kind,
        name: name.map(ToString::to_string),
        value: redact_with_masks(value, masks),
        value_bytes: value.len(),
    }
}

fn is_default_runner_variable(name: &str) -> bool {
    if name.eq_ignore_ascii_case("CI") {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    upper.starts_with("GITHUB_") || upper.starts_with("RUNNER_")
}

#[derive(Clone, Debug)]
struct LogicalLine {
    number: usize,
    text: String,
}

fn logical_lines(text: &str) -> Vec<LogicalLine> {
    let normalized = text.replace("\r\n", "\n");
    normalized
        .split('\n')
        .enumerate()
        .filter_map(|(index, line)| {
            if index == normalized.matches('\n').count() && line.is_empty() {
                None
            } else {
                Some(LogicalLine {
                    number: index + 1,
                    text: line.trim_end_matches('\r').to_string(),
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_assignments_and_heredocs() {
        let analysis = analyze_env_file_with_checks(
            EnvFileKind::Output,
            "ONE=two\nJSON<<EOF\n{\"ok\":true}\nEOF\n",
            Some("GITHUB_OUTPUT".to_string()),
            &[],
        );
        assert_eq!(analysis.records.len(), 2);
        assert_eq!(analysis.records[0].name.as_deref(), Some("ONE"));
        assert_eq!(analysis.records[1].value, "{\"ok\":true}");
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "env_file.output.records")
        );
    }

    #[test]
    fn reports_missing_heredoc_delimiter() {
        let analysis =
            analyze_env_file_with_checks(EnvFileKind::Env, "NAME<<EOF\nvalue\n", None, &[]);
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "env_file.env.heredoc.delimiter")
        );
    }

    #[test]
    fn blocks_node_options() {
        let analysis =
            analyze_env_file_with_checks(EnvFileKind::Env, "NODE_OPTIONS=--inspect\n", None, &[]);
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "env_file.env.node_options")
        );
    }

    #[test]
    fn flags_masked_output_values() {
        let analysis = analyze_env_file_with_checks(
            EnvFileKind::Output,
            "token=s3cr3t\n",
            None,
            &["s3cr3t".to_string()],
        );
        assert_eq!(analysis.records[0].value, "***");
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "env_file.output.masked_value")
        );
    }

    #[test]
    fn rejects_large_step_summary() {
        let huge = "x".repeat(STEP_SUMMARY_LIMIT_BYTES + 1);
        let analysis = analyze_env_file_with_checks(EnvFileKind::StepSummary, &huge, None, &[]);
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "env_file.step_summary.size")
        );
    }
}
