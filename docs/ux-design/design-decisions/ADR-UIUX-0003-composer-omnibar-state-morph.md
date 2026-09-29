# ADR-UIUX-0003: Compact Conversation Uses Composer/Omnibar State Morph

**Status:** Accepted for prototype validation  
**Date:** 2026-09-28

## Context

OM's default compact shell places the Omnibar in the lower reach zone. Inside a conversation, the message composer requires the same region. Keeping both fields persistent would waste space and create message-versus-command ambiguity.

## Decision

1. On compact mobile, only one primary text-entry surface owns the bottom zone at a time.
2. In a conversation, the message composer is the default owner.
3. A dedicated, accessible Omnibar trigger switches the lower surface into Omni mode.
4. The change is represented as a semantic `Transform`, not as two simultaneously active fields.
5. Message drafts and composition context are preserved exactly while Omni mode is active.
6. Omni mode has distinct role, placeholder, controls, result surface, and submission semantics; Omnibar text can never be accidentally sent as a message through parser ambiguity.
7. Exit restores the prior composer state.
8. Gesture invocation may be added as an accelerator but is never the only path.
9. Reduced-motion mode preserves the state change without requiring animated movement.
10. Larger layouts may show composer and global Omnibar concurrently when spatial separation is clear.

## Why this pattern

It best satisfies the combined OM goals of lower-reach ergonomics, speed, calmness, semantic motion, accessibility, cross-platform consistency, and a chat-first surface with minimal persistent chrome.

## Rejected compact defaults

- stacked persistent Omnibar + composer;
- floating always-visible assistant-style Omnibar;
- top-only Omnibar access;
- gesture-only invocation.

## Validation requirement

This remains subject to prototype tests for accidental-mode errors, one-handed reach, draft restoration, keyboard/screen-reader use, reduced motion, and iOS/Android/mobile-PWA transfer.
