# gha-command-proof

`gha-command-proof` validates the GitHub Actions runner command protocol without needing GitHub. It parses workflow commands written to stdout/stderr, validates environment files such as `GITHUB_ENV` and `GITHUB_OUTPUT`, redacts registered masks, and emits a text, JSON, or Markdown receipt.

It is built for offline CI runners, action authors, and tools like `ci-forge` that need to explain whether a local run behaved like a GitHub runner would.

## Install

```powershell
cargo install gha-command-proof --locked
```

## Use

Validate a command stream:

```powershell
gha-command-proof log .\examples\logs\good.log
```

Validate an environment file:

```powershell
gha-command-proof env-file --kind env .\examples\env\GITHUB_ENV
gha-command-proof env-file --kind output .\examples\env\GITHUB_OUTPUT
```

Validate a whole step boundary:

```powershell
gha-command-proof step `
  --log .\examples\logs\good.log `
  --github-env .\examples\env\GITHUB_ENV `
  --github-output .\examples\env\GITHUB_OUTPUT
```

Write JSON or Markdown receipts:

```powershell
gha-command-proof log .\examples\logs\good.log --format json --output receipt.json
gha-command-proof step --log .\examples\logs\good.log --format markdown --output receipt.md
```

Write a redacted copy of the log:

```powershell
gha-command-proof log .\examples\logs\good.log --redacted-log-output redacted.log
```

## What It Checks

- Modern `::command key=value::data` workflow commands.
- Legacy `##[command key=value]data` workflow commands still parsed by GitHub runners.
- Runner escape mappings for command data and properties.
- `add-mask` redaction, including multiline and whitespace-separated mask candidates.
- `stop-commands` suppression and resume-token validation.
- Annotation commands: `notice`, `warning`, and `error`.
- Group balance for `group` / `endgroup`.
- Disabled commands: `set-env` and `add-path`.
- Deprecated commands: `set-output` and `save-state`.
- `GITHUB_ENV`, `GITHUB_OUTPUT`, and `GITHUB_STATE` assignment and heredoc syntax.
- `GITHUB_ENV` restrictions for `NODE_OPTIONS` and default runner variables.
- `GITHUB_OUTPUT` values that were previously registered with `add-mask`.
- `GITHUB_PATH` path records.
- `GITHUB_STEP_SUMMARY` size limit and redacted summary content.

## GitHub Actions

```yaml
jobs:
  gha-command-proof:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: wildmason/gha-command-proof@v1
        with:
          mode: step
          log: step.log
          github-output: github-output.txt
          format: markdown
          output: gha-command-proof.md
```

The action wrapper installs the published crate with `cargo install`. For air-gapped use, install the binary in your runner image and call the CLI directly.

## Exit Codes

The CLI exits `0` when there are no failed checks. Warnings do not fail the run unless `--strict` is passed.

## Receipts

Every run emits a receipt with:

- tool name and version
- checked timestamp
- pass/warn/fail/skip summary
- check list with optional source and line
- parsed command records
- parsed environment-file records

Receipt data is designed to be consumed by offline runners and support-bundle tools. Masked values are redacted before they are rendered.

See [docs/spec.md](docs/spec.md) for protocol scope and [docs/RULES.md](docs/RULES.md) for stable check IDs.

## Limits

`gha-command-proof` validates the command channel and file-command protocol. It does not execute workflows, evaluate expressions, resolve actions, run containers, upload artifacts, or emulate GitHub API services.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
