# GHA Command Proof Protocol Scope

`gha-command-proof` models the command and environment-file boundary between an action step and the GitHub Actions runner.

Primary references:

- GitHub Docs, "Workflow commands for GitHub Actions"
- `actions/toolkit` command escaping implementation
- `actions/runner` `ActionCommand` and `FileCommandManager` implementations

## Command Stream

The parser accepts both runner syntaxes:

```text
::name key=value,key=value::data
##[name key=value;key=value]data
```

Modern command parsing trims leading whitespace and requires the line to start with `::`. Legacy parsing recognizes `##[` anywhere in the line, matching the runner's older parser behavior.

Command names and property names are treated case-insensitively and normalized to lowercase in receipts.

## Escaping

Modern command data unescapes:

```text
%0D -> carriage return
%0A -> line feed
%25 -> percent
```

Modern command properties also unescape:

```text
%3A -> colon
%2C -> comma
```

Legacy command fields unescape:

```text
%3B -> semicolon
%0D -> carriage return
%0A -> line feed
%5D -> closing bracket
%25 -> percent
```

## Command Validation

The first release recognizes:

```text
add-mask
add-matcher
add-path
debug
echo
endgroup
error
group
notice
remove-matcher
save-state
set-env
set-output
stop-commands
warning
```

`set-env` and `add-path` are failed because current GitHub runners disable those stdout commands by default. `set-output` and `save-state` are warned because they are deprecated in favor of environment files.

`stop-commands` switches the analyzer into suppressed mode until the matching `::{token}::` line is seen. Commands inside that region are recorded as suppressed rather than processed.

## Environment Files

`GITHUB_ENV`, `GITHUB_OUTPUT`, and `GITHUB_STATE` accept:

```text
NAME=VALUE
NAME<<DELIMITER
multi
line
DELIMITER
```

Blank lines are ignored. Malformed records fail validation.

`GITHUB_PATH` is parsed as one path record per non-empty line.

`GITHUB_STEP_SUMMARY` is treated as Markdown content and fails above the runner's 1 MiB attachment limit.

## Security Behavior

The analyzer never renders raw `add-mask` values in command records. Later command data, environment-file values, summaries, and redacted log output replace known masks with `***`.

When a value written to `GITHUB_OUTPUT` exactly matches a value previously registered with `add-mask`, the step proof fails. GitHub treats masked values as secrets and does not allow them to be set as outputs.

## Compatibility Limits

The tool does not claim to replace a full runner. It does not execute commands, upload summaries, load problem matcher JSON, resolve container paths, or reproduce every server-side annotation behavior. Its output is a compatibility receipt for the action command and file-command protocol.
