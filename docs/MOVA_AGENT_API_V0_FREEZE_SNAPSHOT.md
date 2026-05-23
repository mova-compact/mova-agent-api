# MOVA Agent API V0 Freeze Snapshot

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`  
Head at freeze: `3ea5357` (`test: add runtime authority boundary guards`)

## 1. Executive status

- V0 maturity: deterministic product skeleton with stabilized schemas, module ownership, HTTP adapter surface, and authority boundary guards.
- Implemented:
  - product definition package (TZ/SPEC/ROADMAP/README)
  - Phase 1-7 skeleton + OpenAPI and validation surface
  - HTTP transport skeleton for V0 public routes
  - boundary guard tests for runtime authority limits
- Intentionally frozen:
  - production auth enforcement
  - durable persistence
  - real external connector side effects
  - orchestration/dynamic routing/agent-owned decisions
- Verdict: `PASS_WITH_WARNINGS`
  - warning context: V0 is intentionally non-production in auth/persistence/connector authority and must be explicitly promoted before expansion.

## 2. Canonical execution path

`agent request -> action model -> policy admission -> flat execution -> connector call -> observation write -> evidence response`

## 3. Repository status

- Schemas: product-local JSON Schemas present and validated:
  - `schemas/action.schema.json`
  - `schemas/request_envelope.schema.json`
  - `schemas/policy_admission.schema.json`
  - `schemas/connector_call.schema.json`
  - `schemas/observation_record.schema.json`
  - `schemas/evidence_response.schema.json`
- Module structure stabilized:
  - `src/request`
  - `src/policy`
  - `src/execution`
  - `src/connectors`
  - `src/observation`
  - `src/evidence`
  - `src/http`
- HTTP transport:
  - route skeletons implemented for V0 surface:
    - `GET /capabilities`
    - `POST /actions/validate`
    - `POST /actions/run`
    - `GET /runs/{run_id}`
    - `GET /runs/{run_id}/evidence`
  - adapter-only boundary preserved.
- OpenAPI:
  - `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml` present and validated.
- Tests:
  - unit, HTTP integration, vertical smoke, and runtime authority boundary guards are present.
- Validation tooling:
  - Rust test pipeline and AJV/Swagger validation commands are wired and required.
- Runtime authority guards:
  - connector side effects explicitly disabled by boundary constant/API
  - in-memory-only retention boundary exposed for testability
  - auth placeholder exposed as non-enforced boundary shape.

## 4. Boundary guarantees

Confirmed for frozen V0:

- no orchestration
- no dynamic routing
- no marketplace behavior
- no visual builder behavior
- no autonomous agent decision-making
- no real connector side effects
- no durable persistence
- HTTP remains adapter-only
- runtime does not own cognition

## 5. Commit history summary

Major progression to freeze:

- `8afc371` docs: initialize MOVA Agent API V0 product package
- `5a0ce2a` feat: add Agent API schema and action model skeleton
- `b7e8ee4` feat: add policy admission skeleton
- `746633b` feat: add flat execution skeleton
- `29a25bd` feat: add connector proxy skeleton
- `61433a3` feat: add observation and evidence skeleton
- `b90ba78` test: add vertical Agent API smoke
- `7a3dfb8` docs: add OpenAPI and V0 validation surface
- `73d0a72` feat: add HTTP transport skeleton
- `9878a72` feat: improve request validation
- `637d476` feat: align run evidence model
- `edf4f4d` feat: unify HTTP validation error contract
- `0b3b73f` test: expand HTTP transport coverage
- `3ea5357` test: add runtime authority boundary guards

## 6. Test and validation status

Required commands for frozen V0:

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

Status at freeze snapshot creation: expected green gate; no boundary-expanding behavior introduced.

## 7. Explicit frozen areas

The following areas are frozen and require explicit promotion decision before implementation:

- production auth
- durable persistence
- real external connector execution
- deployment/runtime coupling
- scheduling/orchestration
- dynamic routing
- autonomous agent authority

## 8. Current architecture shape

Stabilized ownership model:

- request: request envelope parsing/normalization/validation
- policy: deterministic admission decision model
- execution: flat single-action execution flow
- connectors: connector boundary shape with deterministic no-side-effect behavior
- observation: in-process observation record ownership
- evidence: evidence response assembly
- http transport: adapter exposing public API routes to internal modules

## 9. Known limitations

Current V0 skeleton intentionally does not:

- enforce production authentication/authorization policy
- persist runs/evidence beyond process memory
- execute real external connector side effects
- provide orchestration/scheduling/multi-step routing logic
- provide autonomous decision authority or cognition ownership
- provide deployment-specific runtime authority semantics

## 10. Recommended future promotion path

High-level promotion sequence after explicit approval:

1. Auth promotion: introduce enforceable auth model bound to policy admission, not transport ownership.
2. Persistence promotion: add explicit durable retention model for runs/evidence with clear migration boundary.
3. Connector execution promotion: enable controlled real connector side effects behind auditable policy gates.
4. Deployment/runtime model promotion: introduce environment/runtime coupling only through isolated adapters and explicit authority contracts.
