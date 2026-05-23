# MOVA Agent API Cloudflare Deployment Proof V0

## Deployment status

- Verdict: `FAIL` (worker-build host-tooling blocker after WASM graph isolation)
- Target worker name: `mova-agent-api-v0`
- Deployed URL: not available (deploy did not complete)
- Deployed commit target: `719cec9` (with deployment adapter changes prepared locally)

## What was prepared

- Added Cloudflare Worker adapter shell:
  - `src/worker_adapter.rs`
- Isolated Worker feature graph and added explicit Worker alias feature:
  - `Cargo.toml`
- Enabled `cdylib` output for Worker build path:
  - `Cargo.toml`
- Added Wrangler config:
  - `wrangler.toml`

Core business modules and V0 boundaries were not rewritten.

## Commands run

### Local required checks (all green)

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

### Cloudflare auth check (green)

- `npx wrangler whoami`

### Dependency diagnostics

- `cargo tree -i ring`
  - result: no `ring` in MOVA Agent API crate dependency graph.
- `cargo tree --target wasm32-unknown-unknown --features worker`
  - result: feature graph resolves under explicit `worker` feature.
- `cargo build --target wasm32-unknown-unknown --features worker`
  - result: success after isolating non-WASM dependency path.

### Deploy attempts (failed)

- `npx wrangler deploy` (multiple attempts)

## Blocking error summary

Deploy now fails in Wrangler custom build pre-step only:

- Build command:
  - `cargo install -q worker-build && worker-build --release --features worker`
- Failure class:
  - host toolchain/compiler environment failure while compiling `worker-build` dependencies (`ring`)
- Observed symptoms:
  - product build for wasm succeeds (`cargo build --target wasm32-unknown-unknown --features worker`).
  - `ring` appears only while building the external tool `worker-build`, not in MOVA product graph.
  - GNU host path still fails in `gcc.exe` invocation during `ring` C compilation.

This blocker is environment-level, not MOVA Agent API core/runtime logic.

## Smoke test status

Not executed against Cloudflare URL because deployment artifact was not produced.

## Storage behavior/limitation note

- Existing V0 storage adapters remain unchanged:
  - in-memory default
  - local file-backed adapter
- Worker-runtime persistence providers (KV/D1/R2) were intentionally not promoted in this task.

## Boundary status

- Preserved:
  - adapter-only HTTP ownership
  - policy/auth/execution/connectors/observation/evidence/storage ownership split
  - no live connector/provider integrations
  - no orchestration/dynamic routing/autonomous authority
- Not completed:
  - Cloudflare deployment proof run due local build-toolchain blocker

## What remains non-production

- No real external connector calls
- No production auth provider integration
- No cloud storage provider promotion (D1/R2/KV)
- No deployment automation pipeline

## Next recommended promotion

Unblock host toolchain for `worker-build` install (or preinstall trusted `worker-build` binary in CI/runtime image), then re-run:

1. `npx wrangler deploy`
2. Smoke routes:
   - `GET /capabilities`
   - `POST /actions/validate`
   - `POST /actions/run`
   - `GET /runs/{run_id}`
   - `GET /runs/{run_id}/evidence`
