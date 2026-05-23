# MOVA Agent API Persistence Promotion Block V0

Date: 2026-05-23  
Repository: `D:/Projects_MOVA/mova-agent-api`

## Executive summary

Persistence Promotion Block V0 is closed with an explicit storage adapter boundary and deterministic persistence semantics.

Boundary verdict: `PASS_WITH_WARNINGS`

Warnings:

- durable storage is local adapter/stub scope only
- no production database/runtime coupling is introduced
- deployment-managed durability remains a future promotion

## What the persistence block contains

1. Storage adapter boundary:
   - `RunStore` trait
   - `RunSnapshot` persistence contract
   - `StorageError` deterministic error model
2. Run/evidence persistence contract:
   - snapshot stores:
     - `run_id`
     - `evidence`
     - `observations`
3. In-memory adapter preserved as default:
   - `InMemoryRunStore`
4. Local durable adapter:
   - `FileBackedRunStore` (isolated local filesystem adapter)
5. Deterministic failing adapter path:
   - `FailingRunStore`
6. Adapter selection contract:
   - `StorageConfig`
   - `create_run_store`
7. HTTP integration via adapter boundary:
   - `/actions/run` writes snapshot through store
   - `/runs/{run_id}` reads via store
   - `/runs/{run_id}/evidence` reads via store
8. Storage failure error contract:
   - `503` + `storage_unavailable`

## What is contractually durable

- durable retention semantics exist at adapter contract level
- file-backed local adapter provides deterministic durable behavior for local/offline use
- persistence reads/writes are no longer owned by HTTP transport internals

## What remains local/stubbed/default

- default runtime path remains in-memory adapter
- durable adapter is local filesystem only
- no production deployment/runtime storage integration

## Explicitly forbidden (unchanged)

- Cloudflare/D1/R2 coupling
- Postgres/MySQL/Redis production coupling
- deployment-specific storage ownership in core modules
- durable identity/session platform
- connector side-effect expansion
- orchestration/dynamic routing/autonomous authority

## Future production storage promotion path

1. add new provider-specific storage adapter module implementing `RunStore`
2. keep provider coupling isolated from HTTP/request/policy core
3. preserve deterministic `StorageError` contract
4. add conformance tests for new adapter against existing storage suite
5. promote runtime/deployment integration explicitly (separate promotion)

## OpenAPI/docs/examples alignment

- OpenAPI includes `503` storage-unavailable responses for:
  - `POST /actions/run`
  - `GET /runs/{run_id}`
  - `GET /runs/{run_id}/evidence`
- Added schemas/examples:
  - `schemas/run_snapshot_record.schema.json`
  - `examples/run_snapshot_record_minimal.json`

## Validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
