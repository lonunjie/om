# ADR-0091 — Standardize a generic entitlement boundary; keep commerce external

## Status
Accepted — 2026-09-23

## Decision
OM may carry an optional `EntitlementPresentation` that a distributor accepts for pack acquisition/update. It defines no payment rail, pricing, currency/token, tax, refund, merchant, settlement or marketplace operation. Holder binding should be scoped/pseudonymous where possible. Once content is intentionally delivered in an OM conversation, the recipient may render it without presenting the source-pack entitlement.
