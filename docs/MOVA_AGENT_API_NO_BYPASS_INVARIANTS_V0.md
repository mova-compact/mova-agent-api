# MOVA Agent API No-Bypass Invariants V0

## Active invariants

1. Agent never receives provider credentials.
2. Agent never submits raw URL for external execution.
3. Agent never selects arbitrary connector.
4. Agent never selects next step in a contract run.
5. Agent never executes operation outside current admitted step.
6. Agent never executes destructive action in V0.
7. Every side effect must pass through connector side-effect guard.
8. Every side effect must produce observation and evidence.
9. Every external endpoint must resolve from runtime allowlist / endpoint_ref.
10. Every human-gated action must stop until gate is resolved.
11. Gate resolution must be recorded in evidence.
12. Transport never owns final authorization.
13. Connector never owns authorization.
14. Policy never becomes planner/orchestrator.
15. Contract-run state never becomes autonomous agent logic.
16. Agent never submits `connector_id`, `endpoint_ref`, `method`, `target_url`, or `side_effect_intent` for contract-run execution.
17. Connector execution request is built only from `OperationAdmission` and contract step metadata.
18. Every allowed `connector_action` has `contract_run.operation_admitted` observation before side effect.
19. Every `connector_action` evidence includes connector summary and admission summary.
20. Agent never selects transition target.
21. Transition target is resolved only from admitted contract `flow.next`.
22. Client-provided next step is forbidden.
23. Missing transition fails deterministically.
24. Runtime never invents `operation_id` for executable steps.
25. Executable steps without `operation_id` are rejected before run start.
26. Public evidence must not expose resolved provider URLs.
27. Gate evidence must preserve the actual flow step id.
28. Broken transition targets are rejected during contract admission.
29. Transition failure is recorded before error response.
30. Contract-run `connector_action` allows only `none`, `local_only`, or `external_network` side-effect intent.
31. `destructive` side-effect intent remains forbidden in contract-run corridor V0.

## Known V0 limitation

Contract-run corridor in V0 uses fixture/static contract definitions. It does not yet load arbitrary MOVA contract packages from external stores.
