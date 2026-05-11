use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::receipt::{Check, Location};

const MODERN_KEY: &str = "::";
const LEGACY_PREFIX: &str = "##[";

const KNOWN_COMMANDS: &[&str] = &[
    "add-mask",
    "add-matcher",
    "add-path",
    "debug",
    "echo",
    "endgroup",
    "error",
    "group",
    "notice",
    "remove-matcher",
    "save-state",
    "set-env",
    "set-output",
    "stop-commands",
    "warning",
];

const UNSUPPORTED_COMMANDS: &[&str] = &["set-env", "add-path"];
const DEPRECATED_COMMANDS: &[&str] = &["set-output", "save-state"];

#[derive(Clone, Debug)]
pub struct LogAnalysis {
    pub checks: Vec<Check>,
    pub commands: Vec<CommandRecord>,
    pub redacted_log: String,
    pub mask_values: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommandRecord {
    pub line: usize,
    pub syntax: CommandSyntax,
    pub name: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub data: String,
    pub processed: bool,
    pub outcome: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandSyntax {
    Modern,
    Legacy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCommand {
    pub line: usize,
    pub syntax: CommandSyntax,
    pub name: String,
    pub properties: BTreeMap<String, String>,
    pub data: String,
}

#[derive(Default)]
struct CommandStats {
    parsed: usize,
    modern: usize,
    legacy: usize,
    unknown: usize,
    unsupported: usize,
    deprecated: usize,
    masks: usize,
    suppressed: usize,
    annotations: usize,
}

#[derive(Default)]
struct MaskSet {
    values: Vec<String>,
}

impl MaskSet {
    fn add(&mut self, value: &str) {
        for candidate in mask_candidates(value) {
            if !candidate.is_empty() && !self.values.iter().any(|known| known == &candidate) {
                self.values.push(candidate);
            }
        }
        self.values
            .sort_by_key(|value| std::cmp::Reverse(value.len()));
    }

    fn mask(&self, value: &str) -> String {
        let mut redacted = value.to_string();
        for secret in &self.values {
            redacted = redacted.replace(secret, "***");
        }
        redacted
    }

    fn contains_exact(&self, value: &str) -> bool {
        !value.is_empty() && self.values.iter().any(|secret| secret == value)
    }
}

pub fn analyze_command_stream(text: &str, source: Option<String>) -> LogAnalysis {
    let mut checks = Vec::new();
    let mut commands = Vec::new();
    let mut redacted_lines = Vec::new();
    let mut masks = MaskSet::default();
    let mut stats = CommandStats::default();
    let mut group_stack: Vec<(usize, String)> = Vec::new();
    let mut stopped: Option<(String, usize)> = None;

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let parsed = parse_command_line(line, line_number);
        if let Some(command) = parsed.as_ref() {
            stats.parsed += 1;
            match command.syntax {
                CommandSyntax::Modern => stats.modern += 1,
                CommandSyntax::Legacy => stats.legacy += 1,
            }
        }

        if let Some((token, _)) = stopped.clone() {
            if let Some(command) = parsed.as_ref()
                && command.syntax == CommandSyntax::Modern
                && command.name.eq_ignore_ascii_case(&token)
            {
                commands.push(record_from_command(command, true, "resumed", &masks));
                stopped = None;
                redacted_lines.push(masks.mask(line));
                continue;
            }

            if let Some(command) = parsed.as_ref() {
                stats.suppressed += 1;
                commands.push(record_from_command(
                    command,
                    false,
                    "suppressed by stop-commands",
                    &masks,
                ));
            }
            redacted_lines.push(masks.mask(line));
            continue;
        }

        let Some(command) = parsed else {
            redacted_lines.push(masks.mask(line));
            continue;
        };

        validate_command(
            &command,
            &source,
            &mut checks,
            &mut stats,
            &mut masks,
            &mut group_stack,
            &mut stopped,
        );

        let outcome = command_outcome(&command, stopped.as_ref().map(|(_, line)| *line));
        commands.push(record_from_command(&command, true, outcome, &masks));
        redacted_lines.push(masks.mask(line));
    }

    if let Some((token, line)) = stopped {
        checks.push(
            Check::fail(
                "commands.stop_commands.unclosed",
                "workflow commands were stopped and never resumed",
                Some(Location::line(source.as_deref(), line)),
            )
            .with_detail("token", redact_token(&token)),
        );
    }

    for (line, title) in group_stack {
        checks.push(
            Check::warn(
                "commands.group.unclosed",
                "group command was not closed by an endgroup command",
                Some(Location::line(source.as_deref(), line)),
            )
            .with_detail("title", title),
        );
    }

    add_summary_checks(&mut checks, &stats, source.as_deref());

    LogAnalysis {
        checks,
        commands,
        redacted_log: redacted_lines.join("\n"),
        mask_values: masks.values,
    }
}

fn validate_command(
    command: &ParsedCommand,
    source: &Option<String>,
    checks: &mut Vec<Check>,
    stats: &mut CommandStats,
    masks: &mut MaskSet,
    group_stack: &mut Vec<(usize, String)>,
    stopped: &mut Option<(String, usize)>,
) {
    let location = Some(Location::line(source.as_deref(), command.line));
    let name = command.name.as_str();

    if !is_known_command(name) {
        stats.unknown += 1;
        checks.push(Check::warn(
            "commands.unknown",
            format!("unknown workflow command `{name}` will be ignored by GitHub runners"),
            location.clone(),
        ));
        return;
    }

    if UNSUPPORTED_COMMANDS.contains(&name) {
        stats.unsupported += 1;
        checks.push(Check::fail(
            "commands.unsupported",
            format!("`{name}` is disabled on current GitHub runners; use environment files"),
            location.clone(),
        ));
    }

    if DEPRECATED_COMMANDS.contains(&name) {
        stats.deprecated += 1;
        checks.push(Check::warn(
            "commands.deprecated",
            format!("`{name}` is deprecated; use `GITHUB_OUTPUT` or `GITHUB_STATE`"),
            location.clone(),
        ));
    }

    match name {
        "add-mask" => {
            if command.data.trim().is_empty() {
                checks.push(Check::warn(
                    "commands.add_mask.empty",
                    "`add-mask` with an empty value does not register a mask",
                    location,
                ));
            } else {
                stats.masks += 1;
                masks.add(&command.data);
            }
        }
        "stop-commands" => validate_stop_commands(command, source, checks, stopped),
        "group" => group_stack.push((command.line, masks.mask(&command.data))),
        "endgroup" if group_stack.pop().is_none() => {
            checks.push(Check::warn(
                "commands.group.unmatched_endgroup",
                "`endgroup` appeared before any open `group` command",
                location,
            ));
        }
        "endgroup" => {}
        "echo" => {
            let value = command.data.trim();
            if !value.eq_ignore_ascii_case("on") && !value.eq_ignore_ascii_case("off") {
                checks.push(Check::fail(
                    "commands.echo.invalid",
                    "`echo` command value must be `on` or `off`",
                    location,
                ));
            }
        }
        "debug" | "notice" | "warning" | "error" => {
            if matches!(name, "notice" | "warning" | "error") {
                stats.annotations += 1;
                validate_annotation(command, source, checks);
            }
        }
        "set-output" | "save-state" | "set-env" if missing_property(command, "name") => {
            checks.push(Check::fail(
                format!("commands.{name}.name"),
                format!("`{name}` requires a non-empty `name` property"),
                location,
            ));
        }
        "set-output" | "save-state" | "set-env" => {}
        "add-matcher" if command.data.trim().is_empty() => {
            checks.push(Check::warn(
                "commands.add_matcher.path",
                "`add-matcher` should include a problem matcher file path",
                location,
            ));
        }
        "add-matcher" => {}
        "remove-matcher" => {
            let has_owner = !command
                .properties
                .get("owner")
                .is_none_or(|value| value.trim().is_empty());
            let has_file = !command.data.trim().is_empty();
            if has_owner == has_file {
                checks.push(Check::warn(
                    "commands.remove_matcher.selector",
                    "`remove-matcher` should set exactly one of `owner` property or data file path",
                    location,
                ));
            }
        }
        "add-path" if command.data.trim().is_empty() => {
            checks.push(Check::fail(
                "commands.add_path.path",
                "`add-path` requires a non-empty path",
                location,
            ));
        }
        "add-path" => {}
        _ => {}
    }
}

fn validate_stop_commands(
    command: &ParsedCommand,
    source: &Option<String>,
    checks: &mut Vec<Check>,
    stopped: &mut Option<(String, usize)>,
) {
    let token = command.data.trim();
    let location = Some(Location::line(source.as_deref(), command.line));
    if token.is_empty() {
        checks.push(Check::fail(
            "commands.stop_commands.token",
            "`stop-commands` requires a non-empty resume token",
            location,
        ));
        return;
    }
    if token.eq_ignore_ascii_case("pause-logging") || is_known_command(token) {
        checks.push(Check::fail(
            "commands.stop_commands.token",
            "`stop-commands` token collides with a registered runner command",
            location.clone(),
        ));
    } else if token.len() < 16 {
        checks.push(Check::warn(
            "commands.stop_commands.token_entropy",
            "`stop-commands` token should be random and unique for each run",
            location.clone(),
        ));
    }
    *stopped = Some((token.to_string(), command.line));
}

fn validate_annotation(command: &ParsedCommand, source: &Option<String>, checks: &mut Vec<Check>) {
    let location = Some(Location::line(source.as_deref(), command.line));
    for key in ["line", "endline", "col", "endcolumn"] {
        if let Some(value) = command.properties.get(key)
            && value.parse::<u64>().ok().is_none_or(|value| value == 0)
        {
            checks.push(Check::warn(
                "commands.annotation.position",
                format!("annotation property `{key}` should be a positive integer"),
                location.clone(),
            ));
        }
    }

    let line = parse_u64(command.properties.get("line"));
    let end_line = parse_u64(command.properties.get("endline"));
    let col = parse_u64(command.properties.get("col"));
    let end_col = parse_u64(command.properties.get("endcolumn"));

    if end_line.is_some() && line.is_none() {
        checks.push(Check::warn(
            "commands.annotation.end_line_without_line",
            "`endLine` only has an effect when `line` is also set",
            location.clone(),
        ));
    }
    if (col.is_some() || end_col.is_some()) && line.is_none() {
        checks.push(Check::warn(
            "commands.annotation.column_without_line",
            "`col` and `endColumn` only have an effect when `line` is set",
            location.clone(),
        ));
    }
    if let (Some(line), Some(end_line)) = (line, end_line)
        && end_line < line
    {
        checks.push(Check::warn(
            "commands.annotation.end_line_order",
            "`endLine` should not be less than `line`",
            location.clone(),
        ));
    }
    if let (Some(line), Some(end_line), Some(_)) = (line, end_line, col.or(end_col))
        && end_line != line
    {
        checks.push(Check::warn(
            "commands.annotation.column_multiline",
            "`col` and `endColumn` are ignored when `line` and `endLine` differ",
            location.clone(),
        ));
    }
    if let (Some(col), Some(end_col)) = (col, end_col)
        && end_col < col
    {
        checks.push(Check::warn(
            "commands.annotation.end_column_order",
            "`endColumn` should not be less than `col`",
            location,
        ));
    }
}

fn add_summary_checks(checks: &mut Vec<Check>, stats: &CommandStats, source: Option<&str>) {
    let location = source.map(|source| Location::new(Some(source.to_string()), None));

    if stats.parsed == 0 {
        checks.push(Check::skip(
            "commands.parse",
            "no workflow commands found in the stream",
            location.clone(),
        ));
    } else {
        checks.push(
            Check::pass(
                "commands.parse",
                format!("parsed {} workflow commands", stats.parsed),
                location.clone(),
            )
            .with_detail("modern", stats.modern.to_string())
            .with_detail("legacy", stats.legacy.to_string()),
        );
    }

    if stats.legacy == 0 {
        checks.push(Check::pass(
            "commands.syntax",
            "no legacy `##[...]` commands found",
            location.clone(),
        ));
    } else {
        checks.push(Check::warn(
            "commands.syntax.legacy",
            format!(
                "found {} legacy `##[...]` commands; prefer modern `::...::` commands",
                stats.legacy
            ),
            location.clone(),
        ));
    }

    if stats.unknown == 0 {
        checks.push(Check::pass(
            "commands.known",
            "all processed workflow command names are known runner commands",
            location.clone(),
        ));
    }

    if stats.unsupported == 0 {
        checks.push(Check::pass(
            "commands.unsupported",
            "no disabled stdout commands found",
            location.clone(),
        ));
    }

    if stats.deprecated == 0 {
        checks.push(Check::pass(
            "commands.deprecated",
            "no deprecated stdout state/output commands found",
            location.clone(),
        ));
    }

    if stats.suppressed > 0 {
        checks.push(Check::pass(
            "commands.stop_commands.suppressed",
            format!(
                "{} apparent workflow commands were safely suppressed while commands were stopped",
                stats.suppressed
            ),
            location.clone(),
        ));
    }

    if stats.masks > 0 {
        checks.push(Check::pass(
            "commands.add_mask",
            format!("registered {} mask commands", stats.masks),
            location.clone(),
        ));
    }

    if stats.annotations > 0 {
        checks.push(Check::pass(
            "commands.annotations",
            format!("validated {} annotation commands", stats.annotations),
            location,
        ));
    }
}

pub fn parse_command_line(line: &str, line_number: usize) -> Option<ParsedCommand> {
    parse_modern(line, line_number).or_else(|| parse_legacy(line, line_number))
}

fn parse_modern(line: &str, line_number: usize) -> Option<ParsedCommand> {
    let message = line.trim_start();
    let body = message.strip_prefix(MODERN_KEY)?;
    let separator = body.find(MODERN_KEY)?;
    let command_info = &body[..separator];
    let raw_data = &body[separator + MODERN_KEY.len()..];
    let (raw_name, raw_properties) = split_command_info(command_info, ' ');
    let name = normalize_name(raw_name)?;
    let properties = parse_properties(raw_properties, ',', unescape_property);

    Some(ParsedCommand {
        line: line_number,
        syntax: CommandSyntax::Modern,
        name,
        properties,
        data: unescape_data(raw_data),
    })
}

fn parse_legacy(line: &str, line_number: usize) -> Option<ParsedCommand> {
    let prefix = line.find(LEGACY_PREFIX)?;
    let body = &line[prefix + LEGACY_PREFIX.len()..];
    let rb_index = body.find(']')?;
    let command_info = &body[..rb_index];
    let raw_data = &body[rb_index + 1..];
    let (raw_name, raw_properties) = split_command_info(command_info, ' ');
    let name = normalize_name(raw_name)?;
    let properties = parse_properties(raw_properties, ';', unescape_legacy);

    Some(ParsedCommand {
        line: line_number,
        syntax: CommandSyntax::Legacy,
        name,
        properties,
        data: unescape_legacy(raw_data),
    })
}

fn split_command_info(command_info: &str, delimiter: char) -> (&str, Option<&str>) {
    if let Some(index) = command_info.find(delimiter) {
        (
            &command_info[..index],
            Some(command_info[index + 1..].trim()),
        )
    } else {
        (command_info, None)
    }
}

fn normalize_name(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_ascii_lowercase())
    }
}

fn parse_properties(
    raw_properties: Option<&str>,
    delimiter: char,
    unescape: fn(&str) -> String,
) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    let Some(raw_properties) = raw_properties else {
        return properties;
    };

    for property in raw_properties
        .split(delimiter)
        .filter(|part| !part.is_empty())
    {
        let mut parts = property.splitn(2, '=');
        let Some(key) = parts.next().map(str::trim).filter(|key| !key.is_empty()) else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        properties.insert(key.to_ascii_lowercase(), unescape(value));
    }

    properties
}

pub fn escape_data(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

pub fn escape_property(value: &str) -> String {
    escape_data(value).replace(':', "%3A").replace(',', "%2C")
}

pub fn escape_legacy(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(';', "%3B")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(']', "%5D")
}

pub fn unescape_data(value: &str) -> String {
    value
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%25", "%")
}

pub fn unescape_property(value: &str) -> String {
    value
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%3A", ":")
        .replace("%2C", ",")
        .replace("%25", "%")
}

pub fn unescape_legacy(value: &str) -> String {
    value
        .replace("%3B", ";")
        .replace("%0D", "\r")
        .replace("%0A", "\n")
        .replace("%5D", "]")
        .replace("%25", "%")
}

fn record_from_command(
    command: &ParsedCommand,
    processed: bool,
    outcome: impl Into<String>,
    masks: &MaskSet,
) -> CommandRecord {
    let data = if command.name == "add-mask" {
        "***".to_string()
    } else {
        masks.mask(&command.data)
    };

    CommandRecord {
        line: command.line,
        syntax: command.syntax,
        name: command.name.clone(),
        properties: command.properties.clone(),
        data,
        processed,
        outcome: outcome.into(),
    }
}

fn command_outcome(command: &ParsedCommand, stopped_line: Option<usize>) -> &'static str {
    if command.name == "stop-commands" && stopped_line == Some(command.line) {
        "commands stopped"
    } else if UNSUPPORTED_COMMANDS.contains(&command.name.as_str()) {
        "disabled command"
    } else if DEPRECATED_COMMANDS.contains(&command.name.as_str()) {
        "deprecated command"
    } else if is_known_command(&command.name) {
        "processed"
    } else {
        "unknown command"
    }
}

fn is_known_command(name: &str) -> bool {
    KNOWN_COMMANDS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(name))
}

fn missing_property(command: &ParsedCommand, key: &str) -> bool {
    command
        .properties
        .get(key)
        .is_none_or(|value| value.trim().is_empty())
}

fn parse_u64(value: Option<&String>) -> Option<u64> {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
}

fn mask_candidates(value: &str) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    let trimmed = value.trim_matches(['\r', '\n']);
    if !trimmed.is_empty() {
        candidates.insert(trimmed.to_string());
    }
    for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
        candidates.insert(line.to_string());
    }
    for word in value.split_whitespace().filter(|word| !word.is_empty()) {
        candidates.insert(word.to_string());
    }
    candidates.into_iter().collect()
}

fn redact_token(token: &str) -> String {
    let chars = token.chars().collect::<Vec<_>>();
    if chars.len() <= 4 {
        "***".to_string()
    } else {
        let prefix = chars.iter().take(2).collect::<String>();
        let suffix = chars
            .iter()
            .skip(chars.len().saturating_sub(2))
            .collect::<String>();
        format!("{prefix}***{suffix}")
    }
}

pub(crate) fn redact_with_masks(value: &str, masks: &[String]) -> String {
    let mask_set = MaskSet {
        values: masks.to_vec(),
    };
    mask_set.mask(value)
}

pub(crate) fn masks_contain_exact(masks: &[String], value: &str) -> bool {
    let mask_set = MaskSet {
        values: masks.to_vec(),
    };
    mask_set.contains_exact(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_command_with_escaped_properties() {
        let parsed =
            parse_command_line("::warning file=src%2Clib.rs,line=2::hello%0Aworld", 7).unwrap();
        assert_eq!(parsed.line, 7);
        assert_eq!(parsed.syntax, CommandSyntax::Modern);
        assert_eq!(parsed.name, "warning");
        assert_eq!(parsed.properties["file"], "src,lib.rs");
        assert_eq!(parsed.data, "hello\nworld");
    }

    #[test]
    fn parses_legacy_command_anywhere_in_line() {
        let parsed = parse_command_line("prefix ##[error file=src/main.rs;line=3]bad%5D", 1)
            .expect("legacy command parses");
        assert_eq!(parsed.syntax, CommandSyntax::Legacy);
        assert_eq!(parsed.name, "error");
        assert_eq!(parsed.properties["file"], "src/main.rs");
        assert_eq!(parsed.data, "bad]");
    }

    #[test]
    fn redacts_add_mask_value_and_later_log_lines() {
        let analysis =
            analyze_command_stream("::add-mask::s3cr3t\nplain s3cr3t\n::warning::s3cr3t", None);
        assert!(analysis.redacted_log.contains("plain ***"));
        assert!(!analysis.redacted_log.contains("s3cr3t"));
        assert_eq!(analysis.commands[0].data, "***");
        assert_eq!(analysis.commands[1].data, "***");
    }

    #[test]
    fn stop_commands_suppresses_commands_until_resume_token() {
        let analysis = analyze_command_stream(
            "::stop-commands::token-123456789\n::error::ignored\n::token-123456789::\n::warning::real",
            None,
        );
        let suppressed = analysis
            .commands
            .iter()
            .find(|command| command.name == "error")
            .expect("suppressed command was recorded");
        assert!(!suppressed.processed);
        assert_eq!(suppressed.outcome, "suppressed by stop-commands");
        assert!(
            analysis
                .commands
                .iter()
                .any(|command| command.name == "warning")
        );
    }

    #[test]
    fn flags_unsupported_and_deprecated_commands() {
        let analysis = analyze_command_stream(
            "::set-env name=FOO::bar\n::set-output name=result::ok",
            Some("log.txt".to_string()),
        );
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "commands.unsupported")
        );
        assert!(
            analysis
                .checks
                .iter()
                .any(|check| check.id == "commands.deprecated")
        );
    }

    #[test]
    fn escape_round_trip_uses_runner_mappings() {
        let data = "percent %\r\n";
        assert_eq!(unescape_data(&escape_data(data)), data);
        let property = "a:b,c%\r\n";
        assert_eq!(unescape_property(&escape_property(property)), property);
        let legacy = "a;b]\r\n%";
        assert_eq!(unescape_legacy(&escape_legacy(legacy)), legacy);
    }
}
