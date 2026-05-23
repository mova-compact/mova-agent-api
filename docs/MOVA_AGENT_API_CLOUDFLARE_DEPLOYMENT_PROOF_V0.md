# MOVA Agent API Cloudflare Deployment Proof V0

## Deployment status

- Verdict: `PASS_WITH_WARNINGS` (deployment complete on workers.dev; run-status retrieval remains stateless in Worker mode)
- Target worker name: `mova-agent-api-v0`
- workers.dev subdomain chosen: `s-myasoedov81.workers.dev` (account-level existing subdomain)
- Deployed URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- Deployed commit target: `89462c0`

## What was prepared

- Added Cloudflare Worker adapter shell:
  - `src/worker_adapter.rs`
- Isolated Worker feature graph and added explicit Worker alias feature:
  - `Cargo.toml`
- Enabled `cdylib` output for Worker build path:
  - `Cargo.toml`
- Added Wrangler config:
  - `wrangler.toml`
- Updated Wrangler custom build command to avoid reinstalling `worker-build` on each deploy:
  - `worker-build --release --features worker`
- Aligned `worker` crate version with locally available `worker-build`:
  - `worker = "0.7.5"`

Core business modules and V0 boundaries were not rewritten.

## Commands run

### Local required checks (all green)

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`

### Cloudflare auth check (green)

- `npx wrangler whoami`
  - account: `1a59aedce21bfac94cb0f6c8b2da0484`
  - identity: `s.myasoedov81@gmail.com`

### Dependency diagnostics

- `worker-build --version`
  - result: `worker-build 0.7.5` available on host.
- `cargo install worker-build --locked`
  - result: failed on Windows host while compiling `ring` C dependencies.
- `rustup show`
  - result: Windows GNU host toolchain active; wasm target available.
- `rustup target list --installed`
  - result: includes `wasm32-unknown-unknown`.
- `npx wrangler --version`
  - result: `4.85.0`.
- `cargo tree -i ring`
  - result: no `ring` in MOVA Agent API crate dependency graph.
- `cargo tree --target wasm32-unknown-unknown --features worker`
  - result: feature graph resolves under explicit `worker` feature.
- `cargo build --target wasm32-unknown-unknown --features worker`
  - result: success after isolating non-WASM dependency path.

### Deploy attempts

- `npx wrangler deploy` (new account)
  - custom build succeeded.
  - worker upload succeeded.
  - publish succeeded.
  - deployed URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`

## Blocking error summary

Original blocker was in Wrangler custom build pre-step:

- Build command:
  - `cargo install -q worker-build && worker-build --release --features worker`
- Failure class:
  - host toolchain/compiler environment failure while compiling `worker-build` dependencies (`ring`)
- Observed symptoms:
  - product build for wasm succeeds (`cargo build --target wasm32-unknown-unknown --features worker`).
  - `ring` appears only while building the external tool `worker-build`, not in MOVA product graph.
  - GNU host path still fails in `gcc.exe` invocation during `ring` C compilation.

This blocker is environment-level, not MOVA Agent API core/runtime logic.

Previous blocker after deploy-path fix:

- Build/upload status:
  - `worker-build` step: success.
  - Worker upload: success (`Uploaded mova-agent-api-v0`).
- Final publish failure:
  - Cloudflare account has no registered `workers.dev` subdomain.
  - Wrangler onboarding URL:
    - `https://dash.cloudflare.com/b7d21e183c2afcd7e579e750f75e2ca7/workers/onboarding`
- Failure class:
  - account/environment onboarding, not code/dependency/toolchain.

## Smoke test status

Smoke executed against deployed URL:

- `GET /capabilities`
  - status: success
  - result: V0 execution path and capability surface returned.
- `POST /actions/validate` with `examples/agent_request_minimal.json`
  - status: success
  - result: `{"valid": true}`.
- `POST /actions/run` with `examples/agent_request_minimal.json`
  - status: success
  - result: `run_id=run_req_01`, `status=completed`, deterministic connector summary returned.
- `GET /runs/{run_id}` for `run_req_01`
  - status: not found (`run_not_found`).
  - note: current Worker deployment does not preserve cross-request in-memory run store.
- `GET /runs/{run_id}/evidence` for `run_req_01`
  - status: success
  - result: deterministic evidence returned with policy summary and connector result.

Secret-safety and side-effect checks:

- No raw secret material appeared in returned payloads.
- Connector result confirmed deterministic local mode and `side_effect_performed=false`.
- No live external connector side effects observed.

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
  - durable cross-request run retrieval on Worker runtime (still non-promoted persistence provider)

## What remains non-production

- No real external connector calls
- No production auth provider integration
- No cloud storage provider promotion (D1/R2/KV)
- No deployment automation pipeline

## Next recommended promotion

Promote production-grade Worker persistence provider (explicitly scoped promotion, for example KV/D1 adapter block) to make `GET /runs/{run_id}` stable across requests:

1. define provider-specific storage adapter boundary for Worker runtime.
2. keep HTTP adapter-only ownership unchanged.
3. re-run deployed smoke including run-status retrieval consistency.
