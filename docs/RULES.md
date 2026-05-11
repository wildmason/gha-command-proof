# Rules

`gha-command-proof` emits check IDs intended to stay stable within a major version.

## Command Stream

- `commands.parse`: at least one workflow command was parsed, or the stream is skipped as command-free.
- `commands.syntax`: no legacy `##[...]` commands were found.
- `commands.syntax.legacy`: legacy commands were found; this is a warning.
- `commands.known`: processed command names are known GitHub runner commands.
- `commands.unknown`: an unknown workflow command was found; this is a warning because runners ignore unregistered commands.
- `commands.unsupported`: disabled stdout commands such as `set-env` or `add-path` were found; this is a failure.
- `commands.deprecated`: deprecated stdout state/output commands were found; this is a warning.
- `commands.add_mask`: at least one non-empty `add-mask` command registered a mask.
- `commands.add_mask.empty`: an empty `add-mask` command was found; this is a warning.
- `commands.stop_commands.token`: `stop-commands` used an empty or colliding token; this is a failure.
- `commands.stop_commands.token_entropy`: `stop-commands` used a short token; this is a warning.
- `commands.stop_commands.unclosed`: command processing was stopped and never resumed; this is a failure.
- `commands.stop_commands.suppressed`: apparent commands were suppressed while command processing was stopped.
- `commands.group.unmatched_endgroup`: `endgroup` appeared without a matching `group`; this is a warning.
- `commands.group.unclosed`: `group` appeared without a matching `endgroup`; this is a warning.
- `commands.echo.invalid`: `echo` was not set to `on` or `off`; this is a failure.
- `commands.annotation.position`: annotation line/column metadata was not a positive integer; this is a warning.
- `commands.annotation.end_line_without_line`: `endLine` appeared without `line`; this is a warning.
- `commands.annotation.column_without_line`: `col` or `endColumn` appeared without `line`; this is a warning.
- `commands.annotation.end_line_order`: `endLine` was less than `line`; this is a warning.
- `commands.annotation.column_multiline`: column metadata appeared on a multi-line annotation; this is a warning.
- `commands.annotation.end_column_order`: `endColumn` was less than `col`; this is a warning.
- `commands.add_matcher.path`: `add-matcher` did not include a matcher path; this is a warning.
- `commands.remove_matcher.selector`: `remove-matcher` did not set exactly one selector; this is a warning.

## Environment Files

- `env_file.env.records`: `GITHUB_ENV` records were parsed, or the file was empty.
- `env_file.env.name`: an assignment had an empty name; this is a failure.
- `env_file.env.format`: a line did not use assignment or heredoc syntax; this is a failure.
- `env_file.env.heredoc.header`: a heredoc had an empty name or delimiter; this is a failure.
- `env_file.env.heredoc.delimiter`: a heredoc delimiter was not found; this is a failure.
- `env_file.env.node_options`: `GITHUB_ENV` attempted to set `NODE_OPTIONS`; this is a failure.
- `env_file.env.default_variable`: `GITHUB_ENV` attempted to overwrite a default runner variable; this is a warning.
- `env_file.output.records`: `GITHUB_OUTPUT` records were parsed, or the file was empty.
- `env_file.output.masked_value`: an output exactly matched a previously registered mask; this is a failure.
- `env_file.state.records`: `GITHUB_STATE` records were parsed, or the file was empty.
- `env_file.path.records`: `GITHUB_PATH` path records were parsed, or the file was empty.
- `env_file.step_summary.content`: `GITHUB_STEP_SUMMARY` contained Markdown content, or the file was empty.
- `env_file.step_summary.size`: `GITHUB_STEP_SUMMARY` stayed within the runner attachment size limit.

## Strict Mode

`--strict` converts warnings into release-blocking failures at the summary and exit-code level. Individual check statuses remain `warn` in the receipt so the original severity is preserved.
