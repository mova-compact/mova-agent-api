# MOVA Agent API Documentation Block V0

## What was created

- `docs/MOVA_AGENT_API_DOCS_INDEX_V0.md`
  - top-level docs map with explicit user/operator vs internal doc split.
- `docs/MOVA_AGENT_API_V0_HANDBOOK.md`
  - practical operator handbook for common live issues:
    - KV binding issue
    - run_not_found
    - Webhook.site not receiving request
    - validation errors
    - auth/scope denied
    - connector guard denied
    - storage_unavailable
    - request_too_large
    - rate_limited
- README documentation quick links section.
- `package.json` script:
  - `smoke:public` for discoverable smoke execution.

## Intended audience by document

- User/operator-facing:
  - `docs/MOVA_AGENT_API_V0_HANDBOOK.md`
  - `docs/MOVA_AGENT_API_DOCS_INDEX_V0.md`
  - `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
  - `schemas/*`, `examples/*`
  - `scripts/smoke_public_api.ps1`
- Internal/engineering proof:
  - freeze/proof/promotion block docs under `docs/MOVA_AGENT_API_*_PROOF*`, `*_FREEZE*`, `*_BLOCK*`

## How to maintain docs

1. Update OpenAPI first when route/error contract changes.
2. Align schemas/examples with OpenAPI and runtime behavior.
3. Keep handbook troubleshooting sections synchronized with real error codes/details.
4. Update deployment proof after every deploy behavior change.
5. Re-run validation commands before docs commit.

## Internal vs user-facing boundary

- User-facing docs should stay short, symptom-oriented, and command-first.
- Internal docs can keep full implementation/proof history and promotion decisions.
- Do not mix long internal history into operator handbook pages.

## Documentation validation commands

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- `cargo build --target wasm32-unknown-unknown --features worker`
- optional live smoke:
  - `npm run smoke:public`

## Documentation verdict

`PASS`
