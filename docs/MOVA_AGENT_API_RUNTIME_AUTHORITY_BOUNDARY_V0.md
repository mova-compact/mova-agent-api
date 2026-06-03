# MOVA Agent API Runtime Authority Boundary V0

Date: 2026-05-23
Status: active V0 boundary freeze

## 1. Authority ownership

- HTTP transport is adapter only.
- `request` owns request normalization and validation.
- `policy` owns deterministic admission decision shapes.
- `execution` owns flat single-action execution shape only.
- `connectors` owns the side-effect boundary shape but remains no-side-effect in current V0.
- `observation` owns in-process observation records.
- `evidence` owns response assembly.
- `contract_run` owns contract run state and current step pointer.
- `contract_step` owns static step shape.
- `gate` owns human review state.
- `operation_admission` owns step-scoped admission result.
- No module owns cognition, orchestration, dynamic routing, or autonomous decision-making.

## 2. Auth contract

- Current V0 exposes placeholder and production auth contract shapes.
- Authorization decisions remain policy-owned.
- Transport remains metadata pass-through and does not own final authorization.
- Agent-owned route selection is forbidden.
- Client-provided next step is forbidden.
- Client-provided connector override is forbidden.
- Client-provided raw URL is forbidden.

## 3. Persistence boundary policy

- Run/evidence persistence is adapter-owned through the storage boundary.
- Default adapter is in-memory for local/test behavior.
- Local file-backed adapter is allowed as offline durable adapter.
- Production database/runtime storage integration remains forbidden without explicit promotion.

## 4. Connector side-effect guard

- Connector execution is adapter-owned and guarded by side-effect intent.
- Allowed V0 intents are deterministic and local-safe (`none`, `local_only`) by default.
- Real external connector calls remain forbidden in current V0.
- Disallowed intents (`external_network`, `destructive`) fail deterministically.
- Connector adapters keep side-effect intent explicit and testable.

## 5. Stop conditions for future development

- Stop before adding transport-owned authorization.
- Stop before adding deployment-coupled production storage.
- Stop before adding transport-owned runtime config or secret handling.
- Stop before integrating cloud/runtime secret providers without explicit promotion.
- Stop before adding real external connector execution.
- Stop before adding scheduling, orchestration, dynamic routing, or agent-owned decisions.
