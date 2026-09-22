# ADR-0092 — Bots and agents are typed service principals; selected context is the default

## Status
Accepted — 2026-09-23

## Decision
Autonomous bots, agents and LLM services use provider-independent `ServiceID`s and `ServiceControlState`, never synthetic human AccountIDs. `ConversationEvent` authorship is a typed PrincipalRef plus authenticated endpoint. Selected-context invocation is the safe default; continuous subscriptions and full-Room membership require explicit grants, with full membership visibly becoming a plaintext-recipient boundary. Local inference, remote egress, tool calls and automation are separately capability-scoped and revocable.
