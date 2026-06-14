# MOVA_AGENT_API_V0_1_RELEASE_CONTRACT

Status: ACTIVE

Purpose:
Drive MOVA Agent API from current state to first production release (v0.1).

This document is the release authority.
Future tasks must satisfy this contract.
Features outside this contract are out of scope.

---

# 1. Release Goal

Deliver a production-ready controlled execution API.

Canonical flow:

External Agent
→ MOVA API
→ Contract Run
→ Admission
→ Connector Proxy Registry
→ Provider Adapter
→ Evidence

The release is successful when a real external side effect can be executed through this chain without exposing authority, routes, secrets, or provider internals.

---

# 2. Product Boundary

MOVA Agent API is NOT:

* workflow builder
* marketplace
* scheduler
* autonomous agent platform
* provider integration catalog
* orchestration framework

MOVA Agent API IS:

* contract execution gateway
* admission boundary
* connector proxy
* evidence system
* human gate system

---

# 3. Scope Freeze

Everything below is frozen for v0.1.

Do not add:

* new providers
* marketplace
* OAuth
* multi-tenant support
* planner
* dynamic routing
* visual editor
* registry UI
* secret UI
* public contract authoring

No feature expansion allowed.

Only release-hardening work is allowed.

---

# 4. Release Risks

The release is blocked until all risks are closed.

## R1 Auth Risk

Current state:

API surface can be reached without finalized production authentication.

Must become:

X-MOVA-API-KEY authentication.

PASS:

401 missing key
403 invalid key
valid key works
client cannot self-assign scopes

---

## R2 Ownership Risk

Current state:

single-tenant semantics exist implicitly.

Must become:

server-owned tenant_id.

PASS:

tenant_id assigned server-side
tenant_id appears in run
tenant_id appears in evidence
client cannot set tenant_id

---

## R3 Public Mutation Risk

Current state:

contract registration and execution can be confused into one undifferentiated
public mutation surface.

Must become:

strict dual-mode public boundary.

PASS:

tenant execution routes require `X-MOVA-API-KEY`
admin registration route requires `X-MOVA-ADMIN-API-KEY`
tenant key cannot call admin registration route
public secret mutation disabled
public registry mutation disabled

---

## R4 Replay Risk

Current state:

provider side effects can potentially be retried.

Must become:

idempotent side-effect execution.

PASS:

same idempotency key → same result
different key after completion → 409
completed step never executes twice

---

## R5 Secret Leakage Risk

Current state:

provider proxy exists.

Must become:

provable secret isolation.

PASS:

no token in response
no token in evidence
no chat_id in evidence
no provider URL with secrets

---

## R6 Runtime Divergence Risk

Current state:

Worker and native parity improved.

Must become:

provable parity.

PASS:

public routes identical
OpenAPI identical
same error codes
same admission behavior

---

## R7 Provider Proof Risk

Current state:

provider proxy stops at missing secrets.

Must become:

real provider success path.

PASS:

Telegram delivery succeeds
message_id recorded
evidence recorded
no secret leakage

---

## R8 Documentation Drift Risk

Current state:

OpenAPI and docs exist.

Must become:

release-grade documentation.

PASS:

OpenAPI clean
public routes only
release evidence exists
release changelog exists

---

# 5. Public API Definition

Public routes:

GET /health
GET /ready
GET /capabilities

Admin-only route:

POST /contracts/register

Tenant execution routes:

POST /contracts/{contract_id}/runs

GET /contract-runs/{run_id}
GET /contract-runs/{run_id}/next

POST /contract-runs/{run_id}/steps/{step_id}/execute

GET /contract-runs/{run_id}/gates/current
POST /contract-runs/{run_id}/gates/{gate_id}/resolve

GET /contract-runs/{run_id}/evidence

Anything else is internal or lab.

The presence of `POST /contracts/register` does not widen tenant execution
authority. It remains an admin-only route protected by a separate credential
boundary.

---

# 6. Authority Rules

Contract owns route.

Admission owns permission.

Runtime owns state.

Registry owns provider resolution.

Secret store owns credentials.

Human owns gate decisions.

Agent owns only:

* run creation
* current step input

Agent never owns:

* route selection
* provider selection
* connector selection
* secret access

---

# 7. Security Rules

Forbidden:

* connector override
* endpoint override
* provider override
* secret override
* scope escalation
* route injection
* arbitrary URL execution

All failures must fail closed.

---

# 8. Provider Connector Contract

Architecture:

contract
→ connector_ref
→ registry
→ provider
→ operation
→ secret resolver
→ provider adapter

First provider:

telegram/send_message

Telegram is not architecture.

Telegram is first adapter.

Provider-specific logic must remain behind provider boundary.

---

# 9. Evidence Requirements

Evidence must contain:

* tenant_id
* run_id
* contract_id
* status
* admissions
* transitions
* steps
* gates
* provider summary
* idempotency key

Evidence must never contain:

* secrets
* tokens
* chat ids
* provider credentials
* secret values

---

# 10. Storage Rules

v0.1 storage:

Cloudflare KV

Stores:

* run state
* evidence
* observations
* idempotency records

TTL:

7 days

No D1 migration.
No Durable Objects migration.

---

# 11. Release Sequence

Codex must execute work in this order.

Phase 1

Auth hardening.

Deliver:

API key auth.

---

Phase 2

Ownership hardening.

Deliver:

server-owned tenant_id.

---

Phase 3

Public surface hardening.

Deliver:

enforce dual-mode public auth boundaries.

---

Phase 4

Idempotency.

Deliver:

single execution guarantee.

---

Phase 5

Evidence hardening.

Deliver:

complete evidence model.

---

Phase 6

Provider proof.

Deliver:

real Telegram send.

---

Phase 7

OpenAPI cleanup.

Deliver:

public release schema.

---

Phase 8

Release packaging.

Deliver:

release evidence
changelog
v0.1.0 tag

---

# 12. Release Acceptance

v0.1 is released only if all are true.

Auth PASS.

Tenant PASS.

Dual-mode public boundary PASS.

Idempotency PASS.

Evidence PASS.

Provider proof PASS.

Worker parity PASS.

OpenAPI PASS.

Release evidence PASS.

Git tag created:

v0.1.0

If any item fails:

release status = NOT READY.

---

# 13. Codex Execution Policy

Codex must not add new product scope.

Codex must not expand architecture.

Codex must not introduce new providers.

Codex must not introduce marketplace concepts.

Codex must not introduce multi-tenant concepts.

Codex must only close release risks.

Every task must reference:

R1-R8

and explicitly state which release risk it closes.

No task may be accepted unless it moves at least one release risk from OPEN to CLOSED.
