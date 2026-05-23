# Phase 6 Vertical Smoke Notes

Date: 2026-05-23
Phase: 6 - vertical smoke

## Scope applied

- Added minimal request envelope parser/types in `src/request/mod.rs`.
- Added integration smoke test in `tests/vertical_smoke.rs`.
- Smoke covers the canonical V0 path in deterministic local mode:
  - request parse
  - policy admission shape
  - flat execution plan
  - connector call shape
  - observation append
  - evidence response assembly

## Explicitly out of scope

- No API server
- No external connector side effects
- No orchestration or dynamic routing
- No autonomous decision-making logic

## Tests added

- `vertical_smoke_single_action_path` integration test
- request module parse test for minimal envelope
