# BARBERSHOP OWNER REPORT EXECUTION PASS V0

## Contract

- `contract_id`: `barbershop.owner_report.daily.v0`
- source package path: `D:/Projects_MOVA/deploy-template/cloudflare_contract_operator_agent_template_pack_v0/contracts/barbershop.owner_report.daily.v0`

## Package presence check

Verified files exist in the package:

- `manifest.json`
- `flow.json`
- `policy.json`
- `connector_requirements.json`
- `evidence_expectations.json`
- `open_questions.json`
- `registration_note.md`
- `checks/smoke_plan.md`
- `checks/validation_checklist.md`

## Shape validation check

Local validation script checks passed:

- flow entry exists
- all `next.step` targets exist
- terminal targets match manifest terminal outcomes
- flow connector names match `connector_requirements.json`
- evidence markers in contract evidence map are internally consistent with smoke plan

## Runtime contour used

- API base URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- contract execution method used in current Agent API:
  - contract reference passed in `/actions/run` payload context
  - no dedicated `/contracts/register` endpoint in current public Agent API surface

## Commands executed

- `GET /health`
- `GET /ready`
- `GET /capabilities`
- `POST /actions/validate`
- `POST /actions/run` (success path)
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`
- `POST /actions/run` (guard path, unknown `endpoint_ref`)

## Run results

### Success path

- `request_id`: `req_barber_daily_1779699676`
- `run_id`: `run_idem_idem-req_barber_daily_1779699676`
- `terminal outcome`: `completed`
- cross-request lookup:
  - `GET /runs/{run_id}`: success
  - `GET /runs/{run_id}/evidence`: success

Observed evidence summary included:

- connector execution result (`provider=http.generic.v1`, `endpoint_ref=webhook_site_test`, `http_status=200`)
- policy summary (`decision=allow`)
- trace and observation refs

### Guarded path

- request used unknown endpoint ref
- response:
  - HTTP `502`
  - `error.code=connector_execution_failed`
  - details include `connector_code: endpoint_unknown`
- result: guarded deny behavior confirmed

## Compatibility verdict

`BLOCKED_API_COMPATIBILITY`

Reason:

- Current `mova-agent-api` `/actions/run` path does not execute this operator contract package as a first-class flow step-machine with contract-native HUMAN_GATE transitions.
- Current public surface does not expose dedicated contract registration endpoint (`/contracts/register`) for this package style.

## What is proven now

- API health/readiness/capabilities are live.
- Contract-context request can execute end-to-end through:
  - request validation
  - policy admission
  - connector execution
  - observation/evidence persistence
  - run/evidence retrieval across requests
- Guard deny path works for connector endpoint governance.

## What remains blocked

- Contract-native gate path proof for:
  - missing/conflicting data -> `human_gate_escalation`
  - human reject -> `stopped_by_human_gate`
  - contract-level evidence markers:
    - `external_data_fetch_result`
    - `deterministic_metrics_result`
    - `anomaly_or_deviation_check_result`
    - `ai_report_draft`
    - `human_gate_decision_if_required`
    - `telegram_delivery_result`
    - `final_run_status`

These remain blocked until Agent API exposes contract registration + contract flow execution semantics directly.
