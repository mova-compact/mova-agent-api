# MOVA Agent API v0.1 Changelog

Status: `NOT READY`

Release contract authority:

- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`

## Included hardening

### Public contract-run surface

- public release surface is limited to contract-run runtime routes plus `health`, `ready`, `capabilities`
- Cloudflare Worker public adapter exposes contract-run corridor routes
- public Worker surface fails closed on lab/internal routes

### Auth and ownership

- public contract-run runtime requires `X-MOVA-API-KEY`
- invalid key returns `403`
- missing key returns `401`
- server-owned `tenant_id` is injected at runtime
- client cannot override `context.tenant_id`

### Replay safety

- public start supports `Idempotency-Key`
- public step execution supports `Idempotency-Key`
- same key replays the same response
- different key after completed step returns `409 step_already_executed`

### Evidence hardening

- public run status exposes `tenant_id`
- public evidence exposes `tenant_id`
- public evidence exposes `evidence.idempotency_key`
- provider evidence remains summary-only
- no token, no `chat_id`, no provider URL with secret in public evidence

### Provider connector proxy

- provider connector registry is active
- first provider adapter is Telegram `send_message`
- provider connector execution remains behind registry and secret boundaries

## Remaining open release blocker

### R7 Provider Proof Risk

Release is still `NOT READY` because real provider proof is not yet complete.

Current live result:

- public contract-run reaches provider connector dispatch
- execute stops at `connector_secret_missing`
- required Cloudflare secrets are still missing:
  - `TELEGRAM_BOT_TOKEN`
  - `TELEGRAM_OWNER_REPORT_CHAT_ID`

## Release verdict

Do not create tag `v0.1.0` yet.

The release contract still evaluates to:

- `R1` CLOSED
- `R2` CLOSED
- `R3` CLOSED
- `R4` CLOSED
- `R5` CLOSED
- `R6` CLOSED
- `R7` OPEN
- `R8` IN PROGRESS
