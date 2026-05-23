# MOVA Agent API V0 Handbook

## Purpose

Quick operational handbook for users/operators of the deployed MOVA Agent API V0 Worker.

Worker URL:
- `https://mova-agent-api-v0.s-myasoedov81.workers.dev`

## Fast checks

PowerShell:

```powershell
Invoke-RestMethod https://mova-agent-api-v0.s-myasoedov81.workers.dev/health
Invoke-RestMethod https://mova-agent-api-v0.s-myasoedov81.workers.dev/ready
npm run smoke:public
```

## Issue runbook

### KV binding issue

Symptoms:
- `storage_unavailable`
- deploy output does not list `env.MOVA_RUN_STORE`

Checks:
- `npx wrangler deploy` and confirm binding line:
  - `env.MOVA_RUN_STORE (...) KV Namespace`
- verify `wrangler.toml` has `[[kv_namespaces]]` with `binding = "MOVA_RUN_STORE"`

### run_not_found issue

Symptoms:
- `GET /runs/{run_id}` -> `run_not_found`

Checks:
- ensure run was created via `POST /actions/run` with successful response.
- if using idempotency, reuse same key to get deterministic replay id:
  - header `Idempotency-Key: <key>`
  - expected run id shape: `run_idem_<key>`

### Webhook.site not receiving request

Symptoms:
- `/actions/run` fails during webhook flow
- no receipt on Webhook.site

Checks:
- confirm connector context:
  - `connector_id = connector.webhook_site.v1`
  - `side_effect_intent = external_network`
  - `target_url` exactly equals allowlisted runtime value
- confirm Worker runtime var:
  - `MOVA_WEBHOOK_SITE_ALLOWED_URL`
- verify evidence connector summary fields:
  - `provider`, `http_status`, `request_correlation_id`, `run_id`

### validation errors

Symptoms:
- `400 validation_failed`

Checks:
- validate examples locally:
  - `npm run validate:examples`
- inspect schema source:
  - `schemas/request_envelope.schema.json`
  - `schemas/action.schema.json`

### auth/scope denied

Symptoms:
- `403 authorization_failed`

Checks:
- inspect `auth_context` in request and required scopes.
- verify policy summary in evidence:
  - `policy_summary.reason_code`

### connector guard denied

Symptoms:
- `403 connector_execution_failed`
- details include `connector_code: connector_denied` or `connector_target_denied`

Checks:
- connector id is allowlisted for active adapter.
- side-effect intent allowed for adapter.
- target URL matches allowlist for webhook provider.

### storage_unavailable

Symptoms:
- `503 storage_unavailable`

Checks:
- KV binding exists in deploy output.
- no KV namespace/account mismatch in `wrangler.toml`.

### request_too_large

Symptoms:
- `413 request_too_large`

Checks:
- keep body under Worker request limit used by API (`64 KiB`).
- trim payload fields and avoid embedding large blobs.

### rate_limited

Symptoms:
- `429 rate_limited`

Checks:
- verify `source.client_id` is not denylisted via runtime var:
  - `MOVA_RATE_LIMIT_DENY_CLIENTS`

## Useful snippets

### Validate request only

```powershell
$body = Get-Content -Raw examples/agent_request_minimal.json
Invoke-RestMethod -Method Post `
  -Uri "https://mova-agent-api-v0.s-myasoedov81.workers.dev/actions/validate" `
  -ContentType "application/json" `
  -Body $body
```

### Run with idempotency key

```powershell
$body = Get-Content -Raw examples/agent_request_minimal.json
Invoke-RestMethod -Method Post `
  -Uri "https://mova-agent-api-v0.s-myasoedov81.workers.dev/actions/run" `
  -Headers @{ "Idempotency-Key" = "ops-demo-001" } `
  -ContentType "application/json" `
  -Body $body
```

## Escalation path

- Contract mismatch questions: check OpenAPI + schemas first.
- Runtime/deploy anomalies: check deployment proof and latest Worker version.
- Boundary questions: check runtime authority and hardening docs.
