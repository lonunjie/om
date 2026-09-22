# ADR-0084 — ConversationID is the single stable identity of an interactive Room

## Status
Accepted — 2026-09-23

## Decision
OM-native private groups/Rooms use their existing provider-independent `ConversationID` as the group-control and MLS-bound object identifier. A distinct `GroupID` is removed from the current architecture because it represented the same interactive object and would create needless aliasing once groups are linked into Spaces.

“Room” remains the product/control term. `SpaceID` and `PublicationID` remain separate because they identify genuinely different object classes.
