# MOVA Agent API Merge + Deploy Evidence V0

## Merge
- source branch: `feat/contract-run-corridor`
- target branch: `main`
- merged commit / PR: `271719b8edc1c6f8aaaf7a1d78bed483d49767a0` (`271719b fix: allow local-only contract-run side effects`)
- merge method: `git merge --ff-only feat/contract-run-corridor`
- date/time: `2026-06-03 10:24:32 +02:00`

## Pre-merge validation
- `cargo test -j 1`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:examples`: PASS
- `npm run validate:openapi`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:all`: PASS

## Post-merge validation
- `cargo test -j 1`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:examples`: PASS
- `npm run validate:openapi`: PASS
- `NODE_OPTIONS=--max-old-space-size=8192 npm run validate:all`: PASS

## Cloudflare deploy
- worker name: `mova-agent-api-v0`
- environment: default `workers.dev`
- deploy command: `npx wrangler deploy`
- deployed URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- deploy status: PASS
- deployed version id: `9171cd30-3328-4b89-aba1-ad666e6feead`

## Post-deploy smoke
- `/health`: PASS
- `/ready`: PASS
- `/capabilities`: PASS
- `POST /actions/validate` with `examples/agent_request_minimal.json`: PASS (`200`, `{"valid":true}`)
- `POST /actions/run` with `examples/agent_request_minimal.json`: WARN (`403`, `connector_execution_failed`, `connector_id is not allowed`)
- `contract-run smoke`: WARN (`POST /contracts/daily_owner_report_v0/runs` returned `404`)
- notes:
  live Worker was updated successfully, but public `capabilities` still exposes only the flat action-path projection.
  contract-run corridor routes are not available on the deployed Worker surface in this environment.
  repo script `scripts/smoke_contract_run_api.ps1` was not used as authoritative proof because it currently contains a PowerShell parser bug before request execution.

## Boundary verdict
`PASS_WITH_WARNINGS`

## Known limitations
- destructive `side_effect_intent` remains forbidden
- no scheduler
- no marketplace
- no dynamic routing
- no autonomous planning
- no arbitrary provider SDK integrations
- live contract-run execute path currently hits connector policy deny (`endpoint_scope_denied`) before reaching gate flow

## Cloudflare Worker route parity fix
- branch: `fix/cloudflare-contract-run-route-parity`
- commit: `fix: expose contract-run corridor routes in worker`
- deployed version id: `ab63cd45-f0fc-4ab4-9aa8-2bc4c1e4675a`
- health: PASS
- ready: PASS
- capabilities: PASS, `contract_run.supported=true`, `worker_surface=true`
- contract-run start: PASS
  `POST /contracts/daily_owner_report_v0/runs` -> `202`
  `run_id=contract_run_req_contract_001`
- contract-run status: PASS
  `GET /contract-runs/contract_run_req_contract_001` -> `200`
- contract-run next: PASS
  `GET /contract-runs/contract_run_req_contract_001/next` -> `200`
  `step_id=step_001`, `operation_id=op_notify_webhook`
- contract-run execute: WARN
  `POST /contract-runs/contract_run_req_contract_001/steps/step_001/execute` -> `403`
  `connector_execution_failed`, `connector_code: endpoint_scope_denied`
- gate: PASS
  `GET /contract-runs/contract_run_req_contract_001/gates/current` -> `200`
  current result before execution: `{"gate": null}`
- evidence: PASS
  `GET /contract-runs/contract_run_req_contract_001/evidence` -> `200`
- verdict: `PASS_WITH_WARNINGS`

Notes:
- Route parity is fixed at Worker adapter level: contract-run corridor routes no longer return `404` because of missing Worker route match.
- Local `wrangler dev` smoke could not be completed in this environment because the local Workers runtime binary supports compatibility dates only through `2026-05-01`, while this Worker requires `2026-05-23`.

## Contract-run connector scope policy fix
- branch: `fix/cloudflare-contract-run-route-parity`
- commit: `fix: align contract-run connector scope policy`
- changed endpoint refs:
  - flat `/actions/run` continues to use `webhook_site_test`
  - contract-run fixture `daily_owner_report_v0` now uses `webhook_site_contract_run_test`
- action endpoint scope:
  - `webhook_site_test` -> `actions.run`
  - control proof: live `POST /actions/run` with only `contracts.run` returns `403`, `required scope missing: actions.run`
- contract-run endpoint scope:
  - `webhook_site_contract_run_test` -> `contracts.run`
  - live `GET /contract-runs/{run_id}/next` shows `allowed_endpoint_ref=webhook_site_contract_run_test`
- deploy version: `adcf121a-cb7e-41bf-b47a-95da257e1dc4`
- live execute result:
  - `POST /contract-runs/{run_id}/steps/step_001/execute` no longer fails with `endpoint_scope_denied`
  - current live result is `502`, `connector_provider_http_failed`, `http_generic responded with status 404`
- full smoke verdict:
  - `PASS_WITH_WARNINGS`
  - route + scope parity fixed
  - full gate/evidence progression is currently blocked by upstream webhook `404`, not by endpoint scope policy

## Provider connector proxy registry
- branch: `fix/cloudflare-contract-run-route-parity`
- commit: `feat: add provider connector proxy registry`
- registry env: `MOVA_PROVIDER_CONNECTOR_REGISTRY_JSON`
- first provider adapter:
  - provider: `telegram`
  - operation: `send_message`
  - status: `first_adapter`
- contract id: `provider_connector_owner_report_v0`
- secret status: `no`
- deploy version: `355e586f-2bd0-4eba-abe1-5d6aeb897dbc`
- live smoke:
  - `POST /contracts/provider_connector_owner_report_v0/runs` -> `202`
  - `GET /contract-runs/{run_id}/next` -> `200`
  - `allowed_connector_id=provider.connector.v1`
  - `constraints.connector_ref=telegram.owner_report_channel`
  - `constraints.provider=telegram`
  - `constraints.operation=send_message`
  - `POST /contract-runs/{run_id}/steps/send_owner_report/execute` -> `503`
  - error wrapper: `connector_execution_failed`
  - connector detail: `connector_secret_missing`
- verdict:
  - `PASS_WITH_WARNINGS`
  - provider connector registry is wired through contract-run corridor and Worker deploy surface
  - Telegram delivery is blocked only by missing Worker secrets, not by route, scope, or provider-dispatch mismatch

## V0.1 public contract-run hardening pass
- branch: `fix/cloudflare-contract-run-route-parity`
- commit: `feat: harden v0.1 public contract-run surface`
- deployed version: `48bca46a-3ab7-411f-af5b-6fceab12c64a`
- release rows targeted: `R4 Replay Risk`, `R5 Secret Leakage Risk`
- health: PASS
- ready: PASS
- capabilities: PASS
- public auth:
  - missing `X-MOVA-API-KEY` -> `401`
  - invalid `X-MOVA-API-KEY` -> `403`
  - valid key works on live contract-run routes
- public ownership:
  - live start returns `tenant_id=tenant_server_owned`
  - live evidence returns `tenant_id=tenant_server_owned`
  - client `context.tenant_id` remains forbidden
- replay proof:
  - live start with `Idempotency-Key: release-r45-start-001` -> `202`
  - repeated live start with same key -> `202`, `idempotent_replay=true`
  - native test proof: same step key replays same response, different key after completion -> `409 step_already_executed`
- secret leakage proof:
  - public evidence now includes `tenant_id`
  - public evidence now includes `evidence.idempotency_key`
  - public evidence still excludes Telegram token, `chat_id`, and raw provider URL with secret
- live contract-run start:
  - `POST /contracts/provider_connector_owner_report_v0/runs` -> `202`
  - `run_id=contract_run_idem_release-r45-start-001`
- live contract-run status:
  - `GET /contract-runs/contract_run_idem_release-r45-start-001` -> `200`
- live contract-run next:
  - `GET /contract-runs/contract_run_idem_release-r45-start-001/next` -> `200`
  - `allowed_connector_id=provider.connector.v1`
  - `constraints.connector_ref=telegram.owner_report_channel`
- live contract-run execute:
  - `POST /contract-runs/contract_run_idem_release-r45-start-001/steps/send_owner_report/execute` -> `503`
  - wrapper error: `connector_execution_failed`
  - connector detail: `connector_secret_missing`
- live contract-run evidence:
  - `GET /contract-runs/contract_run_idem_release-r45-start-001/evidence` -> `200`
  - evidence payload exposes `tenant_id` and `evidence.idempotency_key=release-r45-start-001`
- verdict:
  - `PASS_WITH_WARNINGS`
  - `R4` closed for public release surface with native tests + live start replay proof
  - `R5` closed for public release surface
  - `R7` remains blocked by missing Cloudflare Telegram secrets

## V0.1 public runtime parity + docs cleanup
- branch: `fix/cloudflare-contract-run-route-parity`
- commit: `release-contract row in progress after replay/evidence hardening`
- risks targeted:
  - `R6 Runtime Divergence Risk`
  - `R8 Documentation Drift Risk`
- native public router proof:
  - public router exposes only release routes
  - `/actions/*`, `/runs/*`, `/contracts/register`, legacy compatibility mutation paths return `404`
- Worker public router proof:
  - public Worker surface still serves only release routes
  - non-public routes remain `404 route_not_found`
- OpenAPI proof:
  - `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml` now documents only public release routes
  - public contract-run routes document `401` missing API key and `403` invalid API key
- release docs:
  - `docs/MOVA_AGENT_API_PUBLIC_RUNTIME_PARITY_V0_1.md` added
  - `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md` added
- release status:
  - still `NOT READY`
  - only open blocker remains `R7` until Telegram secrets are configured and live provider delivery succeeds
