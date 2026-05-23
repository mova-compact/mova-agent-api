# MOVA Agent API Cloudflare Deployment Proof V0

## Deployment status

- Verdict: `PASS` (deployment complete with Cloudflare KV persistence and controlled Webhook.site external connector proof)
- Target worker name: `mova-agent-api-v0`
- workers.dev subdomain chosen: `s-myasoedov81.workers.dev` (account-level existing subdomain)
- Deployed URL: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`
- Deployed commit target: `59314b5` + Webhook.site connector provider promotion changes

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
- Added Cloudflare Worker persistence adapter behind `RunStore`:
  - `CloudflareKvRunStore` (KV binding `MOVA_RUN_STORE`)
- Registered and bound Cloudflare KV namespace:
  - `a276a02220fd43ff938eb4ff6b82e0a7`
- Added Worker runtime var for external connector allowlist:
  - `MOVA_WEBHOOK_SITE_ALLOWED_URL=https://webhook.site/91c77fd2-f847-43ed-9c18-79b5aebc8b95`

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
  - active version: `8aac950f-982c-4ac7-b233-f7c988269521`

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

Smoke executed against deployed URL (separate HTTP requests):

- `GET /capabilities`
  - status: success
  - result: V0 execution path and capability surface returned.
- `POST /actions/validate` with `examples/agent_request_minimal.json`
  - status: success
  - result: `{"valid": true}`.
- `POST /actions/run` with `examples/agent_request_minimal.json` (request_id overridden to `req_kv_1779545603`)
  - status: success
  - result: `run_id=run_req_kv_1779545603`, `status=completed`, deterministic connector summary returned.
- `GET /runs/{run_id}` for `run_req_kv_1779545603`
  - status: success
  - result: matching run status, trace_ref, observation_count.
- `GET /runs/{run_id}/evidence` for `run_req_kv_1779545603`
  - status: success
  - result: deterministic evidence returned with policy summary and connector result.

Webhook.site provider smoke:

- Allowlisted URL: `https://webhook.site/91c77fd2-f847-43ed-9c18-79b5aebc8b95`
- `POST /actions/run` using:
  - `connector_id=connector.webhook_site.v1`
  - `side_effect_intent=external_network`
  - `target_url` equal to allowlisted Webhook.site URL
- API run result:
  - `run_id=run_req_webhook_1779546805`
  - `status=completed`
- Cross-request retrieval:
  - `GET /runs/run_req_webhook_1779546805` -> success
  - `GET /runs/run_req_webhook_1779546805/evidence` -> success
- Evidence connector summary includes:
  - `connector_mode=webhook_site`
  - `provider=webhook.site`
  - `http_status=200`
  - `request_correlation_id=corr:req_webhook_1779546805`
  - `side_effect_performed=true`
- External receipt confirmation:
  - Webhook.site latest request UUID: `4deaac6f-36fc-44d6-8814-06bf5e4297bc`
  - Received body contains matching:
    - `run_id=run_req_webhook_1779546805`
    - `correlation_id=corr:req_webhook_1779546805`

Secret-safety and side-effect checks:

- No raw secret material appeared in returned payloads.
- Webhook connector result is explicit and allowlisted; no credentials were used.
- Live side effect occurred only to configured Webhook.site endpoint.

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
- Completed:
  - cross-request run/evidence lookup through Cloudflare KV-backed `RunStore` boundary

## What remains non-production

- External connector execution is promoted only for one allowlisted Webhook.site endpoint
- No production auth provider integration
- Cloudflare KV promoted only as Worker run/evidence persistence adapter
- No D1/R2 promotion
- No deployment automation pipeline

## Next recommended promotion

Promote next runtime capabilities only if explicitly needed (for example D1 history/query layer), while preserving current `RunStore` boundary:

1. keep KV adapter for simple lookup by `run_id`.
2. introduce D1 only when relational query/history is explicitly required.
3. keep HTTP adapter-only ownership unchanged.
