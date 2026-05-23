# MOVA Agent API Connector Execution Promotion Block V0

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`

## Executive summary

Connector Execution Promotion Block V0 is complete with controlled connector execution semantics behind a provider-agnostic adapter boundary.

Boundary verdict: `PASS_WITH_WARNINGS`

Warnings:

- connector execution remains deterministic/local/offline by default
- no arbitrary external network calls are allowed
- no production connector integrations are included

## What the connector block contains

1. Connector adapter boundary:
   - `ConnectorExecutor` trait
   - `ConnectorExecutionRequest`
   - `ConnectorExecutionResult`
   - `ConnectorExecutionError`
2. Connector execution contract:
   - `ConnectorExecutionConfig` with explicit adapter kind and guard settings
3. Side-effect intent model:
   - `none`
   - `local_only`
   - `external_network`
   - `destructive`
4. Deterministic local adapter:
   - `DeterministicLocalConnectorExecutor`
5. Offline stub adapter:
   - `OfflineStubConnectorExecutor`
6. Deterministic failure adapter:
   - `FailingConnectorExecutor`
7. HTTP run path integration:
   - `/actions/run` executes connector through adapter boundary only
8. Observation/evidence linkage:
   - connector status/response persisted in observation result/metadata
   - evidence includes connector status and connector result summary
9. Error contract:
   - connector denial/failure returns `connector_execution_failed`
   - deterministic status mapping:
     - `403` for guard denial
     - `502` for connector runtime failure

## What connector execution is allowed in V0

- deterministic local connector execution through configured adapter
- local/offline stub connector execution through explicit stub rules
- side-effect intent guard enforcement before connector action

## What remains deterministic/local/test-only

- default adapter is deterministic local
- offline stub adapter is fixture-driven and non-network
- connector responses are synthetic and bounded for testability

## What remains explicitly forbidden

- arbitrary external network calls
- production connector integrations (Stripe/GitHub/Telegram/Google/etc.)
- destructive external side effects
- connector marketplace/dynamic discovery
- connector-owned authorization
- transport-owned connector execution

## Future external connector promotion path

1. add provider-specific connector adapter implementing `ConnectorExecutor`
2. keep adapter isolated from HTTP/request/policy core
3. preserve side-effect intent guard contract
4. add conformance tests for provider adapter vs current connector suite
5. explicitly promote network/credential/runtime semantics in separate block

## Schemas/examples/docs alignment

- `schemas/connector_call.schema.json` includes `side_effect_intent`
- `examples/connector_call_minimal.json` aligned
- `schemas/connector_execution_config.schema.json` added
- `examples/connector_execution_config_minimal.json` added
- OpenAPI `/actions/run` includes connector-related denial/failure responses (`403`, `502`)

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
