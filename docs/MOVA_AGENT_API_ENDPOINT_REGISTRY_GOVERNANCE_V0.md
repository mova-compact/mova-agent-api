# MOVA Agent API Endpoint Registry Governance V0

## Endpoint_ref model

`http.generic.v1` executes only by `endpoint_ref`.
Raw user URL execution is forbidden.

## Multi-endpoint registry format

Runtime registry is an explicit list of endpoint entries. Each entry defines:
- `endpoint_ref`
- `url`
- `allowed_methods`
- `allowed_side_effect_intents`
- `required_scopes`
- `timeout_ms`
- `max_retries`
- `evidence_policy`
- `enabled`

`MOVA_HTTP_ENDPOINT_REGISTRY_JSON` is supported for multi-entry env configuration.
Single-entry env vars remain as backward-compatible fallback.

## Per-endpoint governance rules

Execution checks in order:
1. endpoint exists
2. endpoint enabled
3. side_effect_intent allowed
4. method allowed
5. required scopes satisfied
6. request executes with endpoint-local timeout/retry policy

## Denial taxonomy

- `endpoint_unknown`
- `endpoint_disabled`
- `endpoint_method_denied`
- `endpoint_intent_denied`
- `endpoint_scope_denied`
- `endpoint_config_invalid`

## Evidence policy

Supported policies:
- `summary_only`
- `headers_redacted`
- `body_redacted`
- `status_only`

Evidence remains secret-safe. Connector responses are summarized and redacted according to policy.

## Example endpoint registry entry

```json
{
  "endpoint_ref": "webhook_site_test",
  "url": "https://webhook.site/<id>",
  "allowed_methods": ["POST"],
  "allowed_side_effect_intents": ["external_network"],
  "required_scopes": ["actions.run"],
  "timeout_ms": 10000,
  "max_retries": 0,
  "evidence_policy": "summary_only",
  "enabled": true
}
```

## Still forbidden

- arbitrary URL fetch
- dynamic endpoint discovery
- runtime endpoint registration API
- marketplace/plugin connector discovery
- connector-owned auth decisions
- orchestration/dynamic routing/autonomous authority
