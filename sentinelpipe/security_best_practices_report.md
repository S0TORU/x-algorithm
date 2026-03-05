# Security Best Practices Report

## Executive Summary
A targeted security pass was run for Gazetent (`axum` backend + browser JS frontend). High-impact local secret handling and request hardening gaps were addressed. The current app is safer for local operation and open-source release.

## Findings

### [GZT-001] API key persisted in browser storage (Fixed)
- Severity: High
- Location: `gazetent-web/static/app.js`
- Evidence:
  - Previously persisted `apiKey` inside localStorage payload.
  - Current fix strips stored key and forces empty key on boot.
- Fix applied:
  - `applyFormDefaults` deletes legacy `apiKey` values and clears input.
  - `persistForm` no longer writes `apiKey`.
- Relevant lines:
  - `gazetent-web/static/app.js:246`
  - `gazetent-web/static/app.js:253`
  - `gazetent-web/static/app.js:264`

### [GZT-002] Missing baseline response hardening headers (Fixed)
- Severity: Medium
- Location: `gazetent-web/src/main.rs`
- Fix applied:
  - Added middleware with CSP, `nosniff`, frame deny, referrer policy, and permissions policy.
- Relevant lines:
  - `gazetent-web/src/main.rs:44`
  - `gazetent-web/src/main.rs:170`

### [GZT-003] Unbounded request body size (Fixed)
- Severity: Medium
- Location: `gazetent-web/src/main.rs`
- Fix applied:
  - Added `DefaultBodyLimit::max(1024 * 1024)`.
- Relevant lines:
  - `gazetent-web/src/main.rs:43`

### [GZT-004] Pack path traversal / arbitrary file read risk (Fixed)
- Severity: High
- Location: `gazetent-web/src/main.rs`
- Fix applied:
  - Added canonicalized path guard: pack paths must resolve inside workspace root.
  - Applied to preview and run execution paths.
- Relevant lines:
  - `gazetent-web/src/main.rs:150`
  - `gazetent-web/src/main.rs:118`
  - `gazetent-web/src/main.rs:533`

## Residual Risks
- App has no authn/authz by design; safe default is localhost-only usage.
- If deployed publicly, add auth, TLS, rate limits, and outbound target allow-list.

## Status
- JS syntax check passes.
- `cargo build -p gazetent-web` passes.
