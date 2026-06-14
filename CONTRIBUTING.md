# Contributing To MOVA Agent API

This repository is the runtime and execution-boundary layer of MOVA.

## Scope

Allowed here:

- runtime admission behavior
- contract-run lifecycle behavior
- guarded connector execution
- observation and evidence responses
- auth, secrets, human-gate, and operator runtime surfaces
- OpenAPI and examples for the runtime boundary

Do not add here:

- new core language canon that belongs to `mova-spec`
- new package canon that belongs to `mova-contract-spec`

## Boundary Rules

- `mova-spec` defines valid language artifacts.
- `mova-contract-spec` defines executable package composition.
- `mova-agent-api` defines runtime behavior and product/API boundary.

Keep those layers explicit in docs and code.

## Documentation Rules

- README should stay product- and runtime-oriented.
- Reference docs should optimize for fast operator and agent comprehension.
- OpenAPI, examples, and docs must not contradict each other.

## Validation

Run the safe verification surfaces before proposing changes:

```bash
cargo test
npm install
npm run validate:all
```

Use smoke scripts only when the environment is configured for them.

## Dirty Worktree Rule

This repository often carries active runtime work.

- do not revert unrelated user changes
- do not rewrite proof docs or runtime code unless the task requires it
- prefer additive documentation and navigation improvements when code work is already in progress
