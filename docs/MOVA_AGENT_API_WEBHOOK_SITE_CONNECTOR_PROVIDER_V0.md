# MOVA Agent API Webhook.site Connector Provider V0

## Provider purpose

Provide the first controlled live external connector provider for MOVA Agent API V0 using Webhook.site, with no credentials and explicit side-effect guardrails.

## Endpoint allowlist model

- Adapter kind: `webhook_site`
- Allowed connector id: `connector.webhook_site.v1`
- Allowed side-effect intent: `external_network`
- Allowed endpoint model: exactly one URL in runtime config
  - runtime var: `MOVA_WEBHOOK_SITE_ALLOWED_URL`
  - validation: must be absolute `https://webhook.site/*`
- Any non-allowlisted target is denied deterministically with `connector_target_denied`.

## Request payload model

Connector request includes:
- `target_url`
- `run_id`
- `correlation_id`
- `trace_ref`
- `input` (caller payload)

Outbound payload sent to Webhook.site is reduced to correlation-safe fields:
- `provider`
- `run_id`
- `correlation_id`
- `call_id`
- `connector_id`
- `trace_ref`

## Side-effect guard behavior

- External network execution is denied unless:
  - connector id is allowed
  - side-effect intent is `external_network`
  - `target_url` equals configured allowlisted URL
  - target host remains `webhook.site`
- Missing/invalid target URL -> `connector_request_invalid`
- Non-2xx provider response -> `connector_provider_http_failed`
- Network failure -> `connector_provider_network_failed`

## Evidence mapping

Connector result mapped into evidence includes:
- `connector_mode=webhook_site`
- `provider=webhook.site`
- `http_status`
- `request_correlation_id`
- `run_id`
- `response_preview` (truncated provider response text)
- `side_effect_performed=true`

No credentials or raw secret material are required or emitted for this provider.

## Deployed smoke result

- Worker URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- Allowlisted endpoint:
  - `https://webhook.site/91c77fd2-f847-43ed-9c18-79b5aebc8b95`
- Test request:
  - `request_id=req_webhook_1779546805`
  - `run_id=run_req_webhook_1779546805`
  - `correlation_id=corr:req_webhook_1779546805`
- API checks:
  - `POST /actions/run` -> completed
  - `GET /runs/{run_id}` -> completed
  - `GET /runs/{run_id}/evidence` -> connector summary with `http_status=200`

## Webhook.site external receipt confirmation

- Webhook.site request UUID: `4deaac6f-36fc-44d6-8814-06bf5e4297bc`
- Captured payload contains matching:
  - `run_id=run_req_webhook_1779546805`
  - `correlation_id=corr:req_webhook_1779546805`
  - `connector_id=connector.webhook_site.v1`

## Boundary verdict

`PASS`

## Remaining non-production limitations

- This promotion allows one explicit Webhook.site endpoint only.
- No arbitrary URL fetcher is introduced.
- No provider credentials/secrets are introduced.
- No connector marketplace/discovery is introduced.
- No live integrations beyond Webhook.site are promoted.
- No orchestration/dynamic routing/autonomous agent authority is introduced.
