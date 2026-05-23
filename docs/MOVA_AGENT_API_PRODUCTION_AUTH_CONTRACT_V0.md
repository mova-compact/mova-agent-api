# MOVA Agent API Production Auth Contract Layer (V0)

Date: 2026-05-23

## Purpose

Define deterministic production-auth contract semantics for MOVA Agent API while preserving module ownership and avoiding platform-scale identity expansion.

## Ownership and authority

- HTTP transport may extract and pass auth material/context only.
- Request module owns auth context normalization/validation.
- Policy module owns final authorization decisions.
- Runtime does not delegate authorization authority to transport.

## Verification contract

- Production mode is signaled by `auth_context.mode = "production"` (or corresponding header metadata).
- Deterministic token reference contract:
  - `token_ref = "token://<issuer>/<subject>"`
- Trusted issuer semantics:
  - verifier accepts only configured trusted issuers.
- Verification output is structured:
  - status: `verified | unverified`
  - reason code
  - issuer
  - subject
  - scopes

## Policy authorization semantics

- For `mode != production`:
  - placeholder path remains deterministic and allowed (V0 compatibility mode).
- For `mode = production`:
  - unverified token -> deny (`auth_unverified`)
  - verified token without required scope -> deny (`scope_denied`)
  - verified token with required scope -> allow (`authorized`)

## Error contract

- Authorization failure returns unified API error shape with:
  - `code = authorization_failed`
  - policy reason details
- Validation failure remains `validation_failed`.

## Evidence and traceability

- Authorization result is encoded into policy admission constraints:
  - `constraints.auth_context`
  - `constraints.auth_verification`
- Evidence response remains policy-summary driven and deterministic.

## Non-goals preserved

- No external IdP integration.
- No JWT/OAuth/Auth0/Cloudflare verification.
- No durable session/user store.
- No RBAC platform expansion.
- No orchestration/dynamic routing/autonomous authority.
