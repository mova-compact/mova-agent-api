# MOVA Agent API

`mova-agent-api` is the runtime and execution-boundary repository in the MOVA stack.

It defines how admitted requests and admitted contract runs are executed, observed, and exposed through runtime surfaces.

It does not define:

- core language semantics
- package canon

## Repository Boundary

| Repository | Owns | Does not own |
| --- | --- | --- |
| `mova-spec` | Language validity: schemas, envelopes, catalogs, verbs, actions | Package canon, runtime execution |
| `mova-contract-spec` | Package canon: manifest, flow, classification, runtime bindings | Runtime routes, run lifecycle, auth |
| `mova-agent-api` | Runtime boundary: admission, contract-run lifecycle, connector execution, observation, evidence, HTTP/API surface | Core language canon, package canon |

## Positioning

This repository is the product/runtime layer for controlled agent actions.

It is the place where:

- public and internal runtime routes exist
- contract runs are admitted and advanced
- connector execution is guarded
- observations and evidence are produced

## Canonical Execution Paths

Flat action path:

- agent request
- action model
- policy admission
- flat execution
- connector call
- observation write
- evidence response

Contract-run corridor:

- contract run start
- admitted next operation
- guarded step execution
- observation
- evidence

## Current Status

- runtime V0 / V0.1 working surface
- public contract-run corridor is the product path
- `/actions/run` remains compatibility-only and not the public product contract
- `X-MOVA-API-KEY` gates the v0.1 public surface

## Quick Start For Humans

Recommended reading order:

1. `README.md`
2. `reference/runtime-boundary-guide.md`
3. `reference/contract-run-cheatsheet.md`
4. `docs/README.md`
5. `docs/openapi/MOVA_AGENT_API_OPENAPI_V0.yaml`
6. `operator-guide/README.md`

Validation entry points:

```bash
cargo test
npm install
npm run validate:all
```

Optional smoke:

```bash
powershell -ExecutionPolicy Bypass -File scripts/smoke_public_api.ps1
```

## Quick Start For LLM Agents / Agent Skill

Use this repository when the user is asking about:

- runtime admission
- contract-run lifecycle
- HTTP/API routes
- connector execution boundaries
- auth, human gates, operator behavior
- observation and evidence response

Do not use this repository as the source of truth for:

- `ds.*` / `env.*` language semantics -> `mova-spec`
- package assembly and manifest canon -> `mova-contract-spec`

## Reference Guides

- [Runtime Boundary Guide](reference/runtime-boundary-guide.md)
- [Contract Run Cheatsheet](reference/contract-run-cheatsheet.md)
- [Auth And Gates Patterns](reference/auth-and-gates-patterns.md)
- [Operator API Governance](reference/operator-api-governance.md)
- [Docs Index](docs/README.md)
- [Examples Index](examples/README.md)

## Repository Layout

```text
mova-agent-api/
├── README.md
├── CONTRIBUTING.md
├── src/
├── schemas/
├── tests/
├── examples/
├── docs/
├── operator-guide/
├── reference/
├── scripts/
├── Cargo.toml
└── package.json
```

## Runtime Boundary Rules

1. Runtime executes admitted behavior; it does not redefine language canon.
2. Runtime consumes package canon; it does not redefine package canon.
3. Public product behavior should center on the contract-run corridor.
4. Connector execution must stay guarded and explicit.
5. Evidence and observations are first-class runtime outputs.

Current alignment rule:

- runtime should prefer package-declared `runtime_binding_set` materialization for `EXTERNAL_CALL`
- runtime should treat direct `flow.connector` metadata as compatibility material, not the long-term package authority
- unified connector proxy semantics are frozen in `mova-contract-spec/docs/UNIFIED_CONNECTOR_PROXY_MODEL_v0.md`

## Validation Surface

- `cargo test`
- `npm run validate:examples`
- `npm run validate:openapi`
- `npm run validate:all`
- smoke scripts under `scripts/`

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
