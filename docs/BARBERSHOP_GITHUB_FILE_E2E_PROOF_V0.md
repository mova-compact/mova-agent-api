# BARBERSHOP_GITHUB_FILE_E2E_PROOF_V0

## Scope
`barbershop.owner_report.daily.v0` executed through MOVA Agent API using local repo files only.

No GitHub API, no connector marketplace, no DB/storage layer promotion.

## Local file layout
- `data/barbershop/input/daily_2026-05-25.json`
- `data/barbershop/config/metrics_rules.json`
- `data/barbershop/output/reports/owner_report_2026-05-25.md`
- `data/barbershop/output/audit/run_<run_id>.json`

## Execution behavior
- Read input and rules from deterministic paths.
- Compute metrics deterministically (non-AI):
  - revenue
  - visits
  - average check
  - cancellations/no-shows
  - payment split and staff totals when available
- AI step is wording-only report draft.
- Human gate is required when flags are detected.
- Write report and audit files back to local repo paths.
- Evidence includes `github_file_result.mode = local_repo_files_only`.

## Verified paths
- Guarded run: `waiting_human` -> reject -> `stopped_by_human_gate`
- Guarded run: `waiting_human` -> approve -> `completed`
- Safe deterministic run: `completed`

## Validation commands
- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- `cargo build --target wasm32-unknown-unknown --features worker`

## Verdict
`PASS_WITH_LOCAL_GITHUB_FILE_STUB`
