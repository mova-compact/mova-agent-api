# MOVA Agent API TZ V0

## Purpose

Define the initial product intent for `mova-agent-api` as a clean API product for controlled agent actions.

## Target users

- AI agents
- operator surfaces
- controlled automation clients

## Problem

The ecosystem needs one clean product boundary for agent requests that can be admitted, executed, observed, and returned with evidence without reusing the old runtime/MCP workspace as the source of truth.

## Product scope

- accept controlled agent requests
- map requests into an action model
- admit or reject actions through policy
- run a flat execution path
- call a connector through a narrow boundary
- write observations
- return evidence-oriented responses

## Non-goals

- no orchestration platform
- no dynamic routing
- no marketplace
- no visual builder
- no autonomous agent decisions
- no platform expansion beyond MOVA Agent API V0
- no modification of old repositories
- no implementation of runtime behavior in this package-definition stage

## Target execution path

- agent request -> action model -> policy admission -> flat execution -> connector call -> observation write -> evidence response

## Source-of-truth decision

- `mova-agent-api` is the only active source of truth for the new product
- `mova-spec` and `mova-contract-spec` remain upstream canon
- `mova-api`, `mova-mcp`, proxy fragments, and old runtime code are reference-only

## Reference repo status

- `mova-api`: old runtime workspace, reference-only
- `mova-mcp`: legacy flat-runner / control surface, reference-only
- `mova_sdk`: compact journal and evidence reference, reference-only
- `mova-spec`: upstream language canon
- `mova-contract-spec`: upstream contract package canon
- proxy fragments: embedded in legacy repos, not a standalone repo in this workspace

## V0 success criteria

- the repository is initialized as the product source of truth
- the product boundary is documented clearly
- the execution path is defined
- the public API surface is drafted
- no runtime behavior is implemented yet
- no legacy code is copied into the new product repository

