# MOVA Agent API Cloudflare Deployment Proof V0

## Deployment status

- Verdict: `FAIL` (environment build-tooling blocker)
- Target worker name: `mova-agent-api-v0`
- Deployed URL: not available (deploy did not complete)
- Deployed commit target: `719cec9` (with deployment adapter changes prepared locally)

## What was prepared

- Added Cloudflare Worker adapter shell:
  - `src/worker_adapter.rs`
- Enabled optional Worker feature flags in Rust package:
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

### Deploy attempts (failed)

- `npx wrangler deploy` (multiple attempts)

## Blocking error summary

Deploy fails during custom build step:

- Build command:
  - `cargo install -q worker-build && worker-build --release --features cloudflare_worker`
- Failure class:
  - host toolchain/linker environment failure while compiling `worker-build` dependencies (`ring`)
- Observed symptoms:
  - on GNU chain: `gcc.exe` compile invocation fails inside `ring` build
  - on MSVC chain: `link.exe` invocation fails (non-MSVC linker behavior / missing proper VS toolchain environment)

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

Unblock host build chain for `worker-build` in this environment, then re-run:

1. `npx wrangler deploy`
2. Smoke routes:
   - `GET /capabilities`
   - `POST /actions/validate`
   - `POST /actions/run`
   - `GET /runs/{run_id}`
   - `GET /runs/{run_id}/evidence`
