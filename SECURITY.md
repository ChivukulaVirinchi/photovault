# Security Policy

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Use GitHub's private vulnerability reporting:
[**Report a vulnerability →**](https://github.com/ChivukulaVirinchi/photovault/security/advisories/new)

This sends the report to maintainers privately. We will acknowledge receipt
and coordinate disclosure with you before any details become public.

If you cannot use GitHub Security Advisories, email
`chivukulakmohan@gmail.com` with subject `[Smriti Security]`.
PGP key fingerprint will be published on the project website prior to v1.0.

## Response targets

These are targets, not contractual guarantees:

| Severity                  | Acknowledge | Fix released |
|---------------------------|-------------|--------------|
| Critical (RCE, data loss) | 48 hours    | 14 days      |
| High                      | 7 days      | 30 days      |
| Medium / Low              | 14 days     | next minor   |

Reports outside scope (see below) will be acknowledged and closed without
a fix timeline.

## Scope

In scope:

- Code execution from malicious image files (image-decoder bugs that lead
  to memory unsafety in Smriti's process).
- Path traversal in scanner, reindexer, asset-installer, or thumbnail
  pipelines (any code that builds filesystem paths from external input).
- SQL injection in database queries.
- Sensitive data written to logs, config, or thumbnails (geolocation,
  filenames, embeddings) without user consent.
- Tampering risk in the asset pack download (the bundled installer
  fetches binaries; integrity verification is in scope).
- Single-instance lock bypass leading to concurrent writes against the
  same library database.

Out of scope:

- Bugs in upstream dependencies — please report to those projects directly.
  We will accept a report if Smriti's specific use of the dependency
  amplifies the impact.
- Issues that require physical access to an unlocked device.
- Cloud-related concerns: Smriti has no cloud component by design.
- ONNX model adversarial inputs (out of scope unless they cause memory
  corruption rather than just incorrect predictions).

## Disclosure

We follow coordinated disclosure. Once a fix is released:

- The vulnerability is documented in `CHANGELOG.md` with a CVE if assigned.
- A GitHub Security Advisory is published.
- Reporters are credited unless they request anonymity.

## Supported versions

During the v0.x series only the latest released minor version receives
security fixes. From v1.0 onward we will document a longer-term support
window for the most recent major.
