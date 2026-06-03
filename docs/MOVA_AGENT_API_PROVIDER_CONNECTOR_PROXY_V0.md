# MOVA Agent API Provider Connector Proxy V0

## Purpose

Controlled provider connector execution for contract-run corridor without exposing provider secrets, raw provider URLs, or provider-specific execution knobs to the agent.

## Registry model

Runtime resolves provider connectors from:

- `MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON`

Registry entry shape:

```json
[
  {
    "connector_ref": "telegram.owner_report_channel",
    "provider": "telegram",
    "operation": "send_message",
    "required_scopes": ["contracts.run"],
    "allowed_operations": ["op_send_owner_report"],
    "secret_refs": {
      "bot_token": "TELEGRAM_BOT_TOKEN",
      "chat_id": "TELEGRAM_OWNER_REPORT_CHAT_ID"
    },
    "evidence_policy": "summary_only",
    "enabled": true
  }
]
```

## Connector ref boundary

Contract flow references only:

- `connector.name = provider.connector.v1`
- `connector_ref = telegram.owner_report_channel`

Contract does not contain:

- bot token
- chat id
- secret names chosen by the agent
- raw provider URL

## Provider adapter boundary

Execution path:

`MOVA -> provider connector registry -> provider dispatcher -> provider adapter`

Telegram is not the connector architecture.
Telegram is the first provider adapter behind the provider connector proxy.

## Secret resolver boundary

Secrets are resolved only inside runtime:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_OWNER_REPORT_CHAT_ID`

Cloudflare setup:

```powershell
npx wrangler secret put TELEGRAM_BOT_TOKEN
npx wrangler secret put TELEGRAM_OWNER_REPORT_CHAT_ID
```

## First provider adapter

- provider: `telegram`
- operation: `send_message`
- contract demo: `provider_connector_owner_report_v0`

Payload text comes from `input_payload.text` or fallback:

- `MOVA contract-run notification`

## No-bypass rules

Client must not override:

- `connector_ref`
- `provider`
- `operation`
- `provider_url`
- `telegram_url`
- `target_url`
- `bot_token`
- `token`
- `chat_id`
- `secret_ref`
- `secret_refs`
- `token_secret_ref`
- `chat_id_secret_ref`

## Evidence policy

Public evidence is summary-only and may include:

- `connector_id`
- `connector_ref`
- `provider`
- `operation`
- `response_preview.ok`
- `response_preview.message_id`

Public evidence must not include:

- bot token
- chat id
- raw Telegram API URL
- full Telegram chat object

## Limitations

- first provider adapter only
- Telegram support is `send_message` only
- no media sending
- no dynamic recipient selection
- no scheduling in this connector
