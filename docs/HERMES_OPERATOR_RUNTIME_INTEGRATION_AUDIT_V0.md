# Hermes Operator Runtime Integration Audit V0

Date: 2026-06-08
Repository: `D:\Projects_MOVA\mova-agent-api`
Authority under audit: `MOVA Agent API`

## Purpose

This audit answers one narrow question:

Can `MOVA Agent API` become the execution runtime for Hermes operator
profiles, replacing local CAF execution in the client operator template?

Scope is limited to the actual `mova-agent-api` repository.
Legacy `mova-api`, `mova-mcp`, and CAF are out of scope except as historical
background.

## Executive Verdict

`PASS_WITH_BLOCKERS`

`MOVA Agent API` already provides the right execution shape for Hermes:

- runtime-owned contract-run corridor
- runtime-owned next-step authority
- runtime-owned human gate resolution
- guarded connector execution
- evidence response as canonical runtime artifact
- server-owned `tenant_id`
- public API key boundary
- no-bypass invariants documented and implemented

Hermes cutover for real client business packages is not blocked by the absence
of a registration path.

`mova-agent-api` already has an operator registration bridge for client
contracts from:

- local packaged contracts
- pinned GitHub source contracts
- inline technical smoke payloads

The real distinction is:

- tenant execution corridor stays narrow and contract-run driven
- admin registration is exposed separately on the same public base URL behind a distinct admin credential boundary

## Source Files Used

- `README.md`
- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`
- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`
- `docs/MOVA_AGENT_API_PUBLIC_RUNTIME_PARITY_V0_1.md`
- `docs/MOVA_AGENT_API_V0_CORE_READINESS_FREEZE.md`
- `src/worker_surface.rs`
- `src/worker_adapter.rs`
- `src/auth/mod.rs`
- `src/evidence/mod.rs`

## Canonical Public Runtime Surface

The current public runtime surface is split into:

- admin registration route:
  - `POST /contracts/register`
- tenant execution corridor:
  - `GET /health`
  - `GET /ready`
  - `GET /capabilities`
  - `POST /contracts/{contract_id}/runs`
  - `GET /contract-runs/{run_id}`
  - `GET /contract-runs/{run_id}/next`
  - `POST /contract-runs/{run_id}/steps/{step_id}/execute`
  - `GET /contract-runs/{run_id}/gates/current`
  - `POST /contract-runs/{run_id}/gates/{gate_id}/resolve`
  - `GET /contract-runs/{run_id}/evidence`

Evidence:

- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
- `src/worker_surface.rs`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`

This split surface is exactly the kind of controlled runtime boundary Hermes needs:

- admin profile can register/bind contracts
- tenant profile can execute admitted contracts without gaining publication authority

## Auth Boundary

Public runtime access requires:

- `X-MOVA-API-KEY`

Rules verified in docs and code:

- missing key -> `401`
- invalid key -> `403`
- public client cannot self-assign scopes

Evidence:

- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`
- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`
- `src/auth/mod.rs`

Integration implication for Hermes:

- each Hermes client profile should carry one runtime API credential boundary
- Hermes should never own provider secrets, only runtime API credentials

## Tenant / Client Ownership Model

Current runtime position:

- public runtime assigns `tenant_id` server-side
- client cannot override `context.tenant_id`
- `tenant_id` appears in run status
- `tenant_id` appears in evidence

Evidence:

- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`
- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`
- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`

Integration implication for Hermes:

- one Hermes profile must map to one runtime key/tenant boundary
- client package must treat runtime `tenant_id` as canonical execution tenant id

Important note:

`mova-agent-api` release docs still say "Codex must not introduce
multi-tenant concepts." This does not cancel server-owned `tenant_id`.
It means the v0.1 product should not expand into a generalized multitenant
platform feature set. Hermes integration should therefore bind one runtime
tenant boundary per deployed client, not introduce cross-tenant product logic.

## Stepwise Execution Authority

This is the strongest fit for Hermes.

Runtime already owns:

- run creation
- current step
- next-step authority
- operation admission
- human gate stop/resume
- terminal evidence

Verified flow:

1. Hermes starts run:
   - `POST /contracts/{contract_id}/runs`
2. Hermes checks run state:
   - `GET /contract-runs/{run_id}`
3. Hermes fetches the current admitted step:
   - `GET /contract-runs/{run_id}/next`
4. Hermes executes only that step:
   - `POST /contract-runs/{run_id}/steps/{step_id}/execute`
5. Hermes resolves gate only through runtime:
   - `GET /contract-runs/{run_id}/gates/current`
   - `POST /contract-runs/{run_id}/gates/{gate_id}/resolve`
6. Hermes reads terminal evidence:
   - `GET /contract-runs/{run_id}/evidence`

Evidence:

- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`
- `src/worker_surface.rs`

Integration implication for Hermes:

- Hermes should not submit local `step_complete` semantics of its own
- Hermes should not infer next step
- Hermes should operate as a controlled client of this runtime corridor

## No-Bypass Quality

`mova-agent-api` is materially cleaner than local CAF for production control,
because bypass is explicitly forbidden at the runtime contract level.

Relevant enforced invariants include:

- agent never selects arbitrary connector
- agent never selects next step
- agent never executes outside current admitted step
- agent never submits provider secrets
- runtime resolves connector refs from registry
- runtime owns `tenant_id`
- completed step cannot execute twice with a different idempotency key

Evidence:

- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`

Integration implication for Hermes:

- Hermes tenant profiles can become substantially thinner
- Hermes no longer needs local runtime authority to preserve discipline

## Connector Boundary

Connector execution is already runtime-owned.

Current runtime shape:

- contract step references `connector_ref`
- runtime resolves connector through provider connector registry
- provider adapter runs inside runtime secret boundary
- public evidence stays summary-only
- first provider adapter is Telegram `send_message`

Evidence:

- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_NO_BYPASS_INVARIANTS_V0.md`
- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`
- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`

Integration implication for Hermes:

- Hermes should never own connector ids, URLs, provider URLs, or provider tokens
- Hermes should only provide the current step input payload allowed by runtime

## Evidence Model

Canonical runtime evidence route:

- `GET /contract-runs/{run_id}/evidence`

Evidence payload already includes the right top-level integration fields:

- `run_id`
- `contract_id`
- `tenant_id`
- `status`
- `trace_ref`
- `policy_summary`
- `observation_refs`
- `evidence.steps`
- `evidence.gates`
- `evidence.transitions`
- `evidence.transition_failures`
- `evidence.idempotency_key`

Evidence:

- `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
- `src/evidence/mod.rs`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`

Integration implication for Hermes:

- this endpoint should become the canonical evidence source
- local client workspaces should mirror this evidence, not replace it

## Idempotency / Replay Safety

Public runtime already includes replay protection:

- run start supports `Idempotency-Key`
- step execution supports `Idempotency-Key`
- same key replays same result
- different key after completed step returns `409`

Evidence:

- `docs/MOVA_AGENT_API_CONTRACT_RUN_CORRIDOR_V0.md`
- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`
- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`
- `docs/MOVA_AGENT_API_PUBLIC_RUNTIME_PARITY_V0_1.md`

Integration implication for Hermes:

- Hermes bridge should forward idempotency keys
- local retries must not call runtime blindly without replay awareness

## Current Blockers For Hermes Cutover

### Registration path reality

For Hermes operator use, a registration path already exists.

Verified operator-facing modes:

- `inline_flow_json` for technical smoke
- `local_packaged` for local client repo verification
- `github_source` for pinned client GitHub source

Evidence:

- `operator-guide/13_CONTRACT_REGISTRATION_PLAYBOOK.md`
- `operator-guide/RUNTIME_GAPS_AND_REPAIR_BACKLOG.md`
- `docs/BARBERSHOP_ADMISSION_BRIDGE_LIVE_SMOKE_V0.md`
- `src/http/mod.rs`
- `src/worker_adapter.rs`

So the correct reading is:

- contracts may live beside the operator in a local client repo
- or in a client GitHub repo pinned by `commit_sha`
- they are then registered into `mova-agent-api`
- and executed by runtime from admitted registry state

This is compatible with the Hermes target model.

### Blocker 1: Release still not ready

Current release status:

- `NOT READY`

Open blocker:

- real provider proof not complete

Evidence:

- `docs/MOVA_AGENT_API_V0_1_CHANGELOG.md`

Meaning:

- runtime corridor is structurally correct
- but the first real provider-backed production proof is still open

### Blocker 2: Some registration modes still have limitations

Current known limitations:

- older bridge docs still describe `source_url` as metadata-only in a V0 bridge
- newer operator/runtime docs confirm `github_source` works for pinned public
  GitHub source
- private GitHub source auth-flow is still not implemented

Evidence:

- `docs/CONTRACT_ADMISSION_BRIDGE_V0.md`
- `operator-guide/RUNTIME_GAPS_AND_REPAIR_BACKLOG.md`

Meaning:

- local packaged registration is usable
- pinned public GitHub registration is usable
- private GitHub client repos still need a dedicated auth-flow if that becomes
  your source-of-truth mode

### Blocker 3: V0 still constrained as release-hardening line

Release contract explicitly forbids feature expansion for v0.1.

That means:

- broad new product scope should not be forced into the v0.1 line
- Hermes integration must either:
  - consume the current corridor as-is, or
  - be staged after the release-hardening line is closed

Evidence:

- `docs/MOVA_AGENT_API_V0_1_RELEASE_CONTRACT.md`

## What Is Ready Right Now

Ready now for Hermes integration:

- runtime-backed contract run start
- runtime-backed next-step ownership
- runtime-backed human gate
- runtime-backed evidence retrieval
- runtime-owned connector execution
- runtime-owned tenant assignment
- API key auth boundary
- replay-safe run/step semantics

This is enough to replace local CAF as execution authority for:

- admitted demo contracts
- fixed operator contracts already present in runtime
- constrained pilot execution where contract ids are preloaded into runtime

## What Is Not Ready Yet

Not ready yet in a fully polished sense:

- private GitHub source ingestion auth-flow
- final release closure for real provider proof
- cleanup of older docs that still describe earlier bridge limitations

## Cutover Recommendation

Recommended cutover sequence:

### Phase A — Safe Hermes runtime cutover for admitted contracts

Do now:

- replace tenant CAF execution with `mova-agent-api` execution bridge
- use only existing admitted/runtime-known contracts
- make runtime evidence canonical

### Phase B — Registration path hardening

Do next:

- freeze the exact operator registration boundary in `mova-agent-api`
- keep contract registration/promotion as a deliberate runtime-owned flow
- bind Hermes client packages to runtime contract ids/versions

### Phase C — Full client business package model

Do after Phase B:

- author per-client contracts locally
- publish them into runtime
- run them only by runtime contract id/version

## Integration Decision

Decision for Hermes template work:

- `MOVA Agent API` is the correct runtime target
- CAF should be frozen as legacy local fallback
- Hermes tenant profiles should be refactored toward `MOVA Agent API`
- full production cutover does not need a new publication path invented
- it does need registration-path hardening and runtime release closure

## Final Verdict

`MOVA Agent API` is clean enough to become Hermes execution authority.

It already supports the practical loop:

- author locally or in client GitHub
- register into runtime
- execute through runtime

But this loop still needs release-hardening and registration-path cleanup.

So the correct reading is:

- execution runtime corridor: `READY_FOR_HERMES_CUTOVER`
- client contract lifecycle via registration bridge: `AVAILABLE_WITH_LIMITATIONS`
