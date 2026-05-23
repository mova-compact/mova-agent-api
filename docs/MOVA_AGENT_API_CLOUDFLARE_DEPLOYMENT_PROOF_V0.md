# MOVA Agent API Cloudflare Deployment Proof V0

## Deployment status

- Verdict: `PASS_WITH_WARNINGS` (worker-build path unblocked; publish blocked by Cloudflare account onboarding)
- Target worker name: `mova-agent-api-v0`
- Deployed URL: not available yet (`workers.dev` subdomain is not registered on the account)
- Deployed commit target: `204bcf4` (plus local unblock changes in this task)

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

### Deploy attempts (partially successful)

- `npx wrangler deploy` (multiple attempts)
  - custom build succeeded.
  - worker upload succeeded.
  - publish failed due to missing `workers.dev` subdomain registration on Cloudflare account.

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

Current blocker after deploy-path fix:

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

Not executed against Cloudflare URL because routable `workers.dev` endpoint was not provisioned by Cloudflare account onboarding.

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
  - Public endpoint smoke due missing account `workers.dev` registration

## What remains non-production

- No real external connector calls
- No production auth provider integration
- No cloud storage provider promotion (D1/R2/KV)
- No deployment automation pipeline

## Next recommended promotion

Complete Cloudflare account onboarding by registering a `workers.dev` subdomain, then re-run:

1. `npx wrangler deploy`
2. Smoke routes:
   - `GET /capabilities`
   - `POST /actions/validate`
   - `POST /actions/run`
   - `GET /runs/{run_id}`
   - `GET /runs/{run_id}/evidence`
