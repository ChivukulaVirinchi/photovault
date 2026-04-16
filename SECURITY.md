# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in PhotoVault, please report
it privately by emailing virinchi@gurujada.com.

Please do **not** open a public issue for security vulnerabilities.

We aim to respond within 7 days and provide a fix within 30 days for
critical issues.

## Scope

Issues in scope:
- Code execution from malicious image files
- Path traversal in file operations
- SQL injection in database queries
- Sensitive data leaking from local logs/config

Out of scope:
- Bugs in dependencies (report upstream)
- Issues requiring physical access to the device
- Cloud-related concerns (PhotoVault has no cloud component)

## Disclosure

We follow coordinated disclosure: once a fix is released, the
vulnerability will be documented in `CHANGELOG.md` and credited to the
reporter (if desired).
