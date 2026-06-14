# Runtime Boundary Guide

`mova-agent-api` begins where package canon ends.

It owns:

- route-level runtime behavior
- contract-run lifecycle
- guarded connector execution
- auth and gate handling
- observation and evidence emission

It does not own:

- language semantics from `mova-spec`
- package assembly from `mova-contract-spec`

## Practical Rule

If the question is “how is this request admitted, executed, observed, or exposed through API,” this repository is the source of truth.
