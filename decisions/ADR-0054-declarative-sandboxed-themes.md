# ADR-0054 — User Themes Are Declarative, Sandboxed Data

- **Status:** Accepted
- **Date:** 2026-09-22

## Context

OwnMessenger should permit deep visual personalization without letting theme authors gain code execution, message access, tracking ability or security-UI control.

## Decision

Theme bundles are declarative data plus validated local assets. No executable code, arbitrary HTML/CSS, remote asset beacons or protocol hooks are permitted. Security-critical semantics, focus/accessibility and destructive-state meaning remain reserved to OM.

The PWA uses CSP/Trusted Types and known-safe token compilation as defense in depth.
