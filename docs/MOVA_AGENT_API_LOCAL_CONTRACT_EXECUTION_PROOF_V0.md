# MOVA Agent API Local Contract Execution Proof V0

## Selected source contract

- Source repository: `D:/Projects_MOVA/deploy-template/cloudflare_contract_operator_agent_template_pack_v0`
- Source contract: `contracts/operator_terminal_smoke_v0/flow.json`

## Why this contract was chosen

- It is the smallest deterministic contract in the template pack.
- It has a single deterministic step and terminal completion, so it is safe for local V0 proof.
- It does not require Cloudflare bindings, live connectors, or network side effects.

## What was copied as fixture

- Copied minimal fixture only:
  - `tests/fixtures/operator_terminal_smoke_v0/flow.json`
- No code was copied from deploy-template runtime modules.
- The deploy-template repository was not modified.

## Local registration/loading boundary

- Added local contract boundary module:
  - `src/contracts/mod.rs`
- The module provides:
  - local fixture loader (`LocalContractRegistry`)
  - contract-to-agent-request mapper (`map_contract_to_agent_request`)

## Mapping into MOVA Agent API request/action model

- Contract identity maps into `action.input_payload.contract_id`.
- Contract entry/step metadata maps into:
  - `action.input_payload`
  - `inputs`
  - `context`
- The generated request remains within existing V0 request/action schema semantics.
- Connector context remains deterministic and side-effect-safe:
  - `connector_id = connector.docs.v1`
  - `side_effect_intent = none`

## What was executed locally

- Added integration proof test:
  - `tests/contract_local_execution.rs`
- Proven local path:
  - contract fixture load
  - `/actions/validate`
  - `/actions/run`
  - `/runs/{run_id}`
  - `/runs/{run_id}/evidence`
- Execution remains deterministic and local-only.

## Evidence produced

- Test asserts:
  - run created (`run_req_contract_local_01`)
  - status completed
  - evidence exists in RunStore-backed retrieval
  - connector status completed
  - `side_effect_performed = false`
  - policy decision remains allow

## What remains non-live/stubbed

- No Cloudflare deployment/runtime binding.
- No Telegram send.
- No external network calls.
- No live connector credentials.
- Connector execution remains controlled deterministic adapter behavior.

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
