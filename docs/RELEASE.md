# Release Playbook

## Local Gates

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo doc --locked --no-deps
cargo package --locked
cargo publish --dry-run --locked
```

## Self-Proof

```powershell
cargo run --manifest-path ..\action-proof\Cargo.toml -- --repo-root . --manifest action.yml --strict
cargo run --locked -- log examples\logs\good.log --redacted-log-output target\redacted-good.log
cargo run --locked -- step --log examples\logs\good.log --github-env examples\env\GITHUB_ENV --github-output examples\env\GITHUB_OUTPUT --github-path examples\env\GITHUB_PATH --github-step-summary examples\env\GITHUB_STEP_SUMMARY.md --format json --output target\step-receipt.json
```

## Tag And Publish

```powershell
git tag -a vX.Y.Z -m "gha-command-proof vX.Y.Z"
git push origin main
git push origin vX.Y.Z
cargo publish --locked
```

## Post-Publish

```powershell
cargo install gha-command-proof --version X.Y.Z --locked --force
gha-command-proof --version
gha-command-proof log examples\logs\good.log
```

Create a GitHub Release and run the released-action smoke workflow from `main`.
