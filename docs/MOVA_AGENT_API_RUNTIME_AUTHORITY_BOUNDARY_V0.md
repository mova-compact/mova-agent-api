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
- No module owns cognition, orchestration, dynamic routing, or autonomous decision-making.

## 2. Auth placeholder contract

- Current V0 exposes only an auth placeholder shape.
- Current V0 does not enforce production auth.
- Future auth promotion must remain policy/admission-bound, not transport-owned authority expansion.

## 3. In-memory retention policy

- Current run/evidence state is in-memory only.
- In-memory state is test/demo retention, not durable storage.
- No persistence backend is allowed without explicit promotion decision and tests.

## 4. Connector side-effect guard

- Current connector behavior is deterministic and side-effect-free.
- Real external connector calls are forbidden in current V0.
- Connector adapters must keep side-effect intent explicit and testable.

## 5. Stop conditions for future development

- Stop before adding production auth enforcement.
- Stop before adding durable persistence.
- Stop before adding real external connector execution.
- Stop before adding scheduling, orchestration, dynamic routing, or agent-owned decisions.

