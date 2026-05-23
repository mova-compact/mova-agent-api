# MOVA Agent API Cloudflare Persistence Provider V0

## Purpose

Promote deployed MOVA Agent API V0 from stateless Worker run lookup to operational cross-request run/evidence retrieval, while preserving existing V0 ownership boundaries.

## Scope implemented

- Persistence remains behind `RunStore` boundary.
- Added Worker-specific adapter:
  - `CloudflareKvRunStore` in `src/storage/mod.rs`
- Worker adapter now selects Cloudflare KV when binding exists:
  - binding name: `MOVA_RUN_STORE`
- Existing local adapters preserved:
  - `InMemoryRunStore`
  - `FileBackedRunStore`

## Runtime binding

- `wrangler.toml` includes:
  - `[[kv_namespaces]]`
  - `binding = "MOVA_RUN_STORE"`
  - `id = "a276a02220fd43ff938eb4ff6b82e0a7"`

## Contract behavior

- `POST /actions/run` writes `RunSnapshot` to KV via `RunStore::put_snapshot`.
- `GET /runs/{run_id}` reads snapshot via `RunStore::get_snapshot`.
- `GET /runs/{run_id}/evidence` reads the same persisted snapshot.
- Error contract remains deterministic:
  - storage failures map to `storage_unavailable`.

## Boundary status

- Preserved:
  - HTTP transport is adapter-only.
  - Request/auth/policy/execution semantics unchanged.
  - Connector behavior remains deterministic/no-live-side-effects.
  - No orchestration/dynamic routing/autonomous authority.
- Not introduced:
  - D1/R2/KV beyond run/evidence lookup scope
  - live connector calls
  - production external auth provider integration

## Security and secret handling

- Persisted run snapshots continue to use redacted observation/evidence data path.
- No raw secret material intentionally persisted in public response payloads.

## Verification commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- `cargo build --target wasm32-unknown-unknown --features worker`
- `npx wrangler deploy`

## Deployed smoke proof summary

- URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- Verified path:
  - `POST /actions/run` -> returns `run_id`
  - `GET /runs/{run_id}` -> returns persisted run status
  - `GET /runs/{run_id}/evidence` -> returns persisted evidence

## Next promotion candidate

- Introduce D1-backed adapter only if query/history requirements appear that KV key-value lookup by `run_id` cannot satisfy.
