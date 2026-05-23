# MOVA Agent API Auth Boundary V0

Date: 2026-05-23

## Scope

This document defines the promoted V0 auth boundary for `mova-agent-api` without enabling production authentication.

## Ownership rule

- Auth context is policy-bound input.
- HTTP transport may extract/pass auth metadata.
- HTTP transport is not the authority owner and does not perform final authorization.

## V0 auth model

- `auth_context` is an optional request-level shape.
- `auth_context` fields:
  - `mode`
  - `actor_id`
  - `token_ref`
  - `scopes`
  - `source`
  - `verified`
- In V0, `verified` is placeholder-only and must remain `false`.
- Missing `auth_context` is valid in V0 and resolves to deterministic placeholder semantics.

## Policy behavior in V0

- Policy admission receives normalized auth context.
- Policy stores auth context in admission constraints for traceable policy input.
- Policy emits deterministic placeholder reason codes:
  - `ok_auth_placeholder_none`
  - `ok_auth_placeholder_request`
  - `ok_auth_placeholder_header`
- No production authorization decisioning is introduced.

## HTTP adapter behavior in V0

- HTTP may map optional headers to `auth_context` when payload omits it:
  - `x-mova-auth-mode`
  - `x-mova-actor-id`
  - `x-mova-token-ref`
  - `x-mova-scopes` (comma-separated)
  - `x-mova-auth-source`
- Header metadata is forwarded as policy input only.
- No token verification, identity provider integration, or transport-owned authorization is performed.

## Frozen areas (unchanged)

The following remain frozen and require explicit future promotion:

- production auth enforcement
- real token validation
- external identity provider integration
- durable user/session state
- durable persistence
- real external connector side effects
- deployment/runtime coupling
- orchestration/dynamic routing/autonomous agent authority

## Future promotion gate

Production auth may be introduced only by explicit promotion that defines:

1. authoritative verification contract and trust boundary
2. policy-bound enforcement semantics
3. persistence/session impacts (if any)
4. connector and runtime side-effect implications
