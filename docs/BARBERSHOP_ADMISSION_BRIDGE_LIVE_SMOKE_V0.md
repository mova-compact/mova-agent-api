# BARBERSHOP ADMISSION BRIDGE LIVE SMOKE V0

## Scope

Prove end-to-end execution through the new contract admission bridge for:

- `barbershop.owner_report.daily.v0`

Flow proven:

`register -> inspect -> run -> waiting_human -> decision -> terminal -> evidence`

## Registration proof

Endpoint:

- `POST /contracts/register`

Payload mode:

- `inline_flow_json` (execution mode required in current bridge)
- metadata included:
  - `manifest`
  - `policy`
  - `connector_requirements`
  - `evidence_expectations`
  - `open_questions`

Inspection proof:

- `GET /contracts` includes `barbershop.owner_report.daily.v0`
- `GET /contracts/barbershop.owner_report.daily.v0` returns admitted record

## Run scenarios

Run ids used:

- guarded reject path: `ctrun_barber_guard_01`
- guarded approve path: `ctrun_barber_approve_01`
- deterministic safe path: `ctrun_barber_safe_01`

### Guarded path -> waiting human

Endpoint:

- `POST /contracts/barbershop.owner_report.daily.v0/run`

Input outcomes:

- `fetch_daily_source_data = source_error`

Observed:

- run status: `waiting_human`
- evidence available before continuation via:
  - `GET /runs/ctrun_barber_guard_01/evidence`

### Reject continuation

Endpoint:

- `POST /contracts/runs/ctrun_barber_guard_01/decision`

Decision:

- `reject`

Observed:

- terminal status: `stopped_by_human_gate`
- Telegram delivery not executed (`telegram_delivery_executed=false` in evidence payload)
- evidence preserved

### Approve continuation

Endpoints:

- `POST /contracts/barbershop.owner_report.daily.v0/run` (guarded start)
- `POST /contracts/runs/ctrun_barber_approve_01/decision`

Decision:

- `approve`

Observed:

- terminal status: `completed`
- Telegram delivery step executed (`telegram_delivery_executed=true`)
- evidence finalized

### Deterministic safe path

Endpoint:

- `POST /contracts/barbershop.owner_report.daily.v0/run`

Input outcomes:

- `fetch_daily_source_data = default`
- `validate_data_completeness = default`
- `check_anomalies_and_deviation = default`
- `compose_owner_report_ai = default`
- `deliver_telegram_report = default`

Observed:

- terminal status: `completed`
- no human gate pause
- evidence markers present

## Evidence markers verified

From `GET /runs/ctrun_barber_safe_01/evidence`:

- `external_data_fetch_result`
- `deterministic_metrics_result`
- `anomaly_or_deviation_check_result`
- `ai_report_draft`
- `telegram_delivery_result`
- `final_run_status`

For guarded runs, `human_gate_decision_if_required` is present in continuation evidence.

## Limitations observed

- `source_url` is admitted as metadata; execution still requires `inline_flow_json`.
- Human gate continuation state is in-memory only.
- No persistent continuation across process restart.

## Outcome

`PASS_WITH_LIMITATIONS`
