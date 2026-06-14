# Hermes Dual-Mode Runtime API TZ V0

## Goal

Support a strict Hermes dual-mode architecture:

- `admin` mode for contract registration/binding/update preparation
- `tenant` mode for execution only

## Runtime Contract

The runtime must distinguish:

- admin contract-management operations
- tenant execution operations

## Admin Surface

Admin-only operations should cover:

- `POST /contracts/register`
- future contract update/version publish route if added
- future genetic-layer/admin routes if added

Admin auth requirements:

- separate admin scope or key
- no tenant-only key may call registration routes
- every registration action should be auditable

## Tenant Surface

Tenant-only operations:

- `POST /contracts/{contract_id}/runs`
- `GET /contract-runs/{run_id}`
- `GET /contract-runs/{run_id}/next`
- `POST /contract-runs/{run_id}/steps/{step_id}/execute`
- `GET /contract-runs/{run_id}/gates/current`
- `POST /contract-runs/{run_id}/gates/{gate_id}/resolve`
- `GET /contract-runs/{run_id}/evidence`

Tenant auth requirements:

- tenant key cannot register or update contracts
- tenant key can execute only admitted contracts
- runtime continues to own `tenant_id`

## Required Runtime Behavior

1. Admin key can register contract source.
2. Tenant key cannot register contract source.
3. Tenant key can execute only admitted contract ids.
4. Runtime evidence remains canonical.
5. Registration and execution both stay auditable.

## Current State

- Worker public surface currently exposes tenant execution routes.
- Worker public surface now exposes `POST /contracts/register` behind a separate admin header/key path.
- Tenant execution routes remain on the tenant key path.
- Native repo implementation and Worker surface are aligned for admin registration.

## Recommendation

Preferred product model:

- public tenant execution surface stays narrow
- admin registration surface is exposed separately but explicitly
- auth/scopes are different for admin and tenant modes
