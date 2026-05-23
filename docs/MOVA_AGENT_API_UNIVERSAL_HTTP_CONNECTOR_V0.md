# MOVA Agent API Universal HTTP Connector V0

## Purpose
This block promotes connectors from provider-specific shape (`connector.webhook_site.v1`) to a controlled universal HTTP connector boundary (`connector.http.generic.v1`) while preserving V0 safety controls.

## Why this is not an arbitrary fetch proxy
- Connector execution is allowed only for runtime-configured `endpoint_ref` entries.
- Raw user-provided URL execution is not allowed in `http.generic.v1`.
- Method set is explicitly constrained to `GET` and `POST` in V0.
- Side-effect intent must remain `external_network` and be allowlisted.

## Endpoint reference model
Runtime endpoint registry defines trusted targets:
- `endpoint_ref`
- resolved `url` (`https` only)
- `allowed_methods`
- `allowed_side_effect_intents`

Request shape (concept):
- `provider`: `http.generic.v1` (via `connector_id: connector.http.generic.v1`)
- `endpoint_ref`: runtime key
- `method`: `GET | POST`
- optional `headers`
- payload in `input_payload` / connector `body`

## Runtime allowlist model
- Runtime config owns endpoint registry (`ConnectorExecutionConfig.endpoint_registry`).
- Worker runtime can load endpoint config from env:
  - `MOVA_HTTP_ENDPOINT_REF`
  - `MOVA_HTTP_ENDPOINT_URL`
  - `MOVA_HTTP_ENDPOINT_ALLOWED_METHODS`

## Secret-safe request model
- Connector inputs and outputs pass through existing redaction boundary.
- Evidence stores response summary only (`status`, preview, attempts, timeout/retry metadata).
- Secret-like fields are redacted; raw secret values are not emitted.

## Guard behavior
- Deny on unknown `endpoint_ref` (`connector_endpoint_denied`).
- Deny on disallowed method (`connector_method_denied`).
- Deny on non-allowlisted side-effect intent (`connector_side_effect_denied`).
- Fail deterministically on upstream/network/timeout failures with connector error mapping.

## Evidence mapping
Connector result includes:
- `provider: http.generic.v1`
- `endpoint_ref`
- `resolved_url`
- `method`
- `http_status`
- `attempts`, `max_retries`, `timeout_ms`
- `response_preview`

## Live smoke proof
- Deployed path uses Worker adapter + runtime endpoint allowlist.
- Smoke flow:
  1. `POST /actions/run` with `connector.http.generic.v1` and `endpoint_ref`.
  2. `GET /runs/{run_id}` cross-request lookup.
  3. `GET /runs/{run_id}/evidence` connector summary verification.
  4. External receipt verified on Webhook.site using correlation/run id.

## Intentionally forbidden
- Arbitrary URL fetch.
- Open proxy behavior.
- Dynamic connector marketplace/discovery.
- Connector-owned auth or policy decisions.
- Orchestration/dynamic routing/autonomous authority.
- Live provider SDK integrations beyond explicit allowlisted HTTP endpoint calls.
