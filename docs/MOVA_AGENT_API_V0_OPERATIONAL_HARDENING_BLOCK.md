# MOVA Agent API V0 Operational Hardening Block

## Status

- Verdict: `PASS_WITH_WARNINGS`
- Scope: operational safety hardening before operator-facing integrations.

## Implemented hardening items

1. Health/readiness surface
- Added `GET /health`
- Added `GET /ready`

2. Request/body guard
- Worker request body limit enforced at `64 KiB`.
- Oversized requests return `413 request_too_large`.

3. Connector timeout policy
- `ConnectorExecutionConfig.timeout_ms` added.
- Default `timeout_ms=10000`.
- Exposed in evidence connector summary.

4. Connector retry policy
- `ConnectorExecutionConfig.max_retries` added.
- Default `max_retries=0` (no retry).
- Retry loop is explicit for retryable provider failures.

5. Idempotency contract for `POST /actions/run`
- Added `Idempotency-Key` support.
- Deterministic run id for keyed requests: `run_idem_<sanitized-key>`.
- Replays return existing run snapshot through storage boundary.

6. Abuse/rate-limit guard boundary
- Added deterministic guard contract via runtime denylist:
  - `MOVA_RATE_LIMIT_DENY_CLIENTS` (comma-separated `source.client_id` values).
- Denied client requests return `429 rate_limited`.

7. KV retention/TTL policy
- Cloudflare KV adapter supports optional TTL:
  - `MOVA_RUN_TTL_SECONDS`
- Current deployment sets `86400` seconds.

8. Error taxonomy cleanup
- Added/standardized operational error shapes for:
  - `request_too_large` (413)
  - `rate_limited` (429)
  - `connector_execution_failed` (403/502/504 mapped by connector failure class)
  - `storage_unavailable` (503)

9. Public smoke script
- Added:
  - `scripts/smoke_public_api.ps1`
- Covers:
  - `/health`
  - `/ready`
  - `/capabilities`
  - `/actions/validate`
  - `/actions/run` (with optional `Idempotency-Key`)
  - `/runs/{run_id}`
  - `/runs/{run_id}/evidence`

10. Live-readiness freeze artifact
- This document is the V0 operational hardening close-out.

## Runtime vars in current Worker deployment

- `MOVA_WEBHOOK_SITE_ALLOWED_URL`
- `MOVA_RUN_TTL_SECONDS=86400`
- `MOVA_RATE_LIMIT_DENY_CLIENTS=""`

## Required checks executed

- `cargo test` ✅
- `npm run validate:examples` ✅
- `npm run validate:openapi` ✅
- `npm run validate:all` ✅
- `cargo build --target wasm32-unknown-unknown --features worker` ✅
- `npx wrangler deploy` ✅
- public smoke against deployed URL ✅

## Warnings / remaining limitations

- Rate-limit guard is deterministic/config-boundary level, not distributed quota accounting.
- Timeout policy is explicit in contract/config and evidence surface, but deep runtime-level network cancellation remains provider/runtime constrained.
- Idempotency uses deterministic keyed run id replay over current KV snapshot model; stronger conflict semantics are a future promotion if needed.

## Boundary status

- Preserved:
  - HTTP adapter-only ownership
  - policy/auth ownership
  - connector adapter ownership and allowlist guard
  - storage boundary ownership
- Not introduced:
  - orchestration/dynamic routing
  - platform/operator integrations
  - new connector providers
