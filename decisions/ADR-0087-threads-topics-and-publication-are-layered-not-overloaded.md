# ADR-0087 — Threads, Topics, private announcements and public Publications are layered distinct semantics

## Status
Accepted — 2026-09-23

## Decision
Replies/threads remain lightweight relations inside one ConversationID; persistent Topics are room-local organization and not sub-Rooms/MLS groups. A private announcement channel is an ordinary private Room with restricted posting capability.

A large public broadcast feed is a separate signed `Publication` object with non-canonical subscriber lists and optional linkage to an independent discussion Room. OM V1 does not invent a separate massive-private-broadcast cryptosystem.
