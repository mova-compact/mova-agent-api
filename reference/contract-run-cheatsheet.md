# Contract Run Cheatsheet

Primary public product corridor:

- `POST /contracts/{contract_id}/runs`
- `GET /contract-runs/{run_id}`
- contract-run next-operation and evidence routes

Compatibility-only path:

- `/actions/run`

## Runtime Rules

- run state owns step order
- runtime executes only the current admitted operation
- connector execution stays guarded
- evidence reflects runtime outcomes, not speculative intent

## Upstream Boundaries

- step semantics come from package canon
- runtime does not invent contract structure
