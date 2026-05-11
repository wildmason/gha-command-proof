# Security Policy

Report security issues privately to the Wildmason maintainers.

Do not open a public issue for vulnerabilities involving secret redaction, receipt leakage, command parsing bypasses, or unsafe environment-file handling.

`gha-command-proof` treats workflow logs and environment files as untrusted input. Receipts should not render raw `add-mask` values, and tests should cover any change that touches redaction.
