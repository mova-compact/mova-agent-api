# MOVA Agent API Auth Verifier Adapter Contract (V0)

Date: 2026-05-23

## Purpose

Define a provider-agnostic external verifier adapter boundary and explicit trust configuration contract, without integrating real identity providers.

## Adapter boundary

- Core abstraction: `AuthVerifier` trait.
- Core product depends on verifier contract only, not provider SDKs/protocols.
- Current implementation remains deterministic and local:
  - no network calls
  - no OAuth/JWT remote validation
  - no external IdP coupling

## Trust configuration contract

`AuthTrustConfig` defines explicit trust rules:

- `verifier_kind` (currently supported: `deterministic_local`)
- `trusted_issuers`
- `trusted_audiences`
- `allowed_scopes`

Validation behavior:

- unsupported/missing config yields deterministic unverified adapter path
- invalid config is surfaced through authorization failure reasoning

## Token reference contract

Deterministic production token reference format:

- `token://<issuer>/<subject>?aud=<audience>`

Verification checks:

1. token shape parseability
2. trusted issuer membership
3. trusted audience membership (if configured)
4. scope filtering against `allowed_scopes`

## Ownership guarantees

- HTTP transport only extracts/passes auth metadata.
- Policy remains final authorization owner.
- Verifier only produces verification result; it does not decide run execution.

## Failure semantics

- Invalid verifier config -> explicit unverified verification result.
- Unverified/unauthorized policy decision -> unified `authorization_failed` response.
- Authorization reasons stay deterministic and traceable in policy constraints/evidence summary.

## Future provider integration path

Future provider adapters must:

1. implement `AuthVerifier` without changing policy ownership
2. remain isolated behind adapter boundary
3. keep trust config explicit and testable
4. avoid leaking provider-specific semantics into request/policy core types
