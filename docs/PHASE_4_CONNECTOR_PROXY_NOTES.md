# Phase 4 Connector Proxy Notes

Date: 2026-05-23
Phase: 4 - connector proxy skeleton

## Scope applied

- Added `ConnectorCall` shape in `src/connectors/mod.rs`.
- Added connector call status enum:
  - `pending`
  - `allowed`
  - `blocked`
  - `failed`
  - `completed`
- Added deterministic `build_connector_call` helper for local skeleton use.

## Explicitly out of scope

- No real external connector invocation
- No network calls
- No dynamic routing
- No marketplace connector behavior

## Tests added

- Unit test for pending connector-call shape.
- Unit test for status enum wire-format stability.
