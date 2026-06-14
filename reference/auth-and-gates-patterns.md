# Auth And Gates Patterns

Use this repository for runtime auth and human-gate behavior.

## Covered Concerns

- API key boundary
- auth verifier adapters
- human approval gates
- guarded connector execution
- operator-visible deny and evidence paths

## Not Covered Here

- language-level policy vocabulary from `mova-spec`
- package-level binding declarations from `mova-contract-spec`

## Practical Heuristic

If a rule changes who may run, approve, or observe something at runtime, it belongs here.
