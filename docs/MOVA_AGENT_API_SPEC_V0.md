# MOVA Agent API Spec V0

## Overview

This document defines the initial product shape for `mova-agent-api` V0. It is a product-local specification that references upstream MOVA canon rather than replacing it.

## Action model

An action is the atomic unit of controlled agent work.

Minimum action model fields:

- `action_id`
- `action_type`
- `target_kind`
- `input_ref` or inline input payload
- `policy_context`
- `connector_context`
- `trace_ref`

The action model must remain compatible with the upstream `action_signature` and the contract/package rules defined by MOVA canon.

## Request envelope

The request envelope is the inbound product-facing shape for controlled agent requests.

Minimum envelope fields:

- `request_id`
- `actor`
- `source`
- `action`
- `inputs`
- `context`
- `correlation`
- `timestamps`

The request envelope must be narrow and admission-friendly. It should not embed execution logic.

## Policy admission model

Policy admission determines whether an action may proceed.

Minimum admission fields:

- `admission_id`
- `action_id`
- `decision`
- `reason_code`
- `policy_version`
- `constraints`
- `allowed_scopes`
- `blocked_scopes`

Admission outcomes:

- `allow`
- `deny`
- `require_review`
- `redact`

Policy admission is deterministic at the boundary and must not become orchestration.

## Connector call model

Connector calls are the controlled external effect boundary.

Minimum connector call fields:

- `connector_id`
- `call_id`
- `request`
- `auth_context`
- `policy_result`
- `status`
- `response`
- `timing`

Connector calls must be gated, auditable, and narrow. The connector layer is not a general plugin system.

## Observation record model

Observation records capture what happened during execution.

Minimum observation fields:

- `run_id`
- `step_id`
- `event_type`
- `timestamp`
- `subject`
- `result`
- `metadata`
- `evidence_ref`

Observations must support traceability without exposing unnecessary internal detail.

## Evidence response model

The evidence response is the final product response returned to the caller.

Minimum evidence fields:

- `run_id`
- `status`
- `result`
- `evidence`
- `trace_ref`
- `observation_refs`
- `policy_summary`

The response should be suitable for audit and downstream operator surfaces.

## Public API surface draft

- `GET /capabilities`
- `POST /actions/validate`
- `POST /actions/run`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`

Draft intent:

- `GET /capabilities` returns supported action types, policy modes, and surface metadata
- `POST /actions/validate` validates request and action shape without execution
- `POST /actions/run` executes one admitted action through the flat path
- `GET /runs/{run_id}` returns run status and observation summary
- `GET /runs/{run_id}/evidence` returns the evidence response

## Internal module shape

- `request`
- `policy`
- `execution`
- `connectors`
- `observation`
- `evidence`
- `schemas/contracts`

Module intent:

- `request` handles envelope parsing and action modeling
- `policy` handles admission logic and decision serialization
- `execution` handles the flat execution loop
- `connectors` handles connector invocation adapters
- `observation` handles write paths for run records
- `evidence` handles final response assembly
- `schemas/contracts` holds product-local shapes that reference upstream canon

## Canonical boundaries

- `mova-spec` remains the language canon
- `mova-contract-spec` remains the contract package canon
- `mova-agent-api` owns the product boundary and the runtime-facing API surface for V0

