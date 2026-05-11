# Contributing

`gha-command-proof` is intentionally small and conformance-focused. Changes should either improve compatibility with GitHub Actions runner behavior, improve receipt clarity, or make the CLI easier to use in offline CI systems.

Before opening a change, run:

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo doc --locked --no-deps
```

When changing parser behavior, add a fixture or unit test that explains the runner behavior being modeled.
