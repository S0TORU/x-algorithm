# Security Policy

## Scope
This policy applies to the `sentinelpipe/` workspace and `gazetent-web` console.

## Supported usage model
- Intended to run locally (`127.0.0.1`) for development/testing.
- Not designed as a public internet service in current form.

## Current security controls
- Local bind only by default.
- Response hardening headers:
  - `Content-Security-Policy`
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
  - `Permissions-Policy` restrictions
- Request body size limit: 1 MB.
- Pack path restriction: pack files must resolve inside workspace root.
- Secrets handling:
  - API keys are redacted from saved artifacts.
  - API keys are not persisted in browser local storage.

## Responsible disclosure
If you find a security issue:
1. Do not post exploit details publicly first.
2. Open a private report with reproduction steps, impact, and affected files.
3. If private reporting is unavailable, open a GitHub issue with minimal details and ask maintainers for a secure channel.

Include:
- Version/commit
- Exact endpoint or UI path
- Minimal proof-of-concept
- Suggested mitigation (optional)

## Hardening guidance for deployments
If you expose this beyond localhost, add:
- Authentication and authorization on all API routes.
- TLS termination and secure proxy policy.
- Network allow-lists for outbound model targets.
- Audit logging and rate limiting.
- Secret manager integration (do not pass raw keys in UI).
