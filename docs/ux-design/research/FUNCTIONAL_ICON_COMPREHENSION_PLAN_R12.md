# Functional-icon comprehension plan — Revision 12

**Evidence class:** test plan and scoring framework; no human results yet.

## Why this exists

OM deliberately uses icons as part of a self-revealing interaction language, but designer familiarity cannot substitute for user comprehension. ISO 9186-1 specifies methods for measuring how well a graphical symbol communicates its intended message and explicitly recognizes supplementary text when a symbol cannot reliably stand alone.

Source: https://www.iso.org/standard/59226.html (2014 edition; a replacement draft is under development in 2026).

## Priority test set

The R12 fixture prioritizes 20 symbols whose misunderstanding would be costly or whose OM meaning is novel/central:

`Omnibar, Spaces, Reply, Forward, Archive, Mute, Pin, Verify identity, Device, Recovery, Pending, Delivered, Read, Retry, Scan QR, Invite, Voice call, Video call, Attach, Search`.

## Protocol

1. Recruit people who did not design OM.
2. Randomize icon order.
3. Show one icon without label or product explanation.
4. Ask an open-ended meaning question before any multiple-choice prompt.
5. Record confidence separately from correctness.
6. Reveal the intended meaning only after the answer is committed.
7. Score responses with a pre-agreed rubric by at least two reviewers for ambiguous answers.
8. Record **critical confusions** separately from merely vague answers (for example, confusing Verify Identity with Delete/Block is more serious than saying “security”).
9. Repeat important symbols in realistic context because context can materially change comprehension.
10. Test across age/experience groups when feasible; accessibility participants should not be treated as an afterthought.

## Scoring

R12 stores raw answers. Recommended analysis categories:

- `2` — exact or operationally equivalent meaning;
- `1` — partial/related but not sufficient for icon-only use;
- `0` — wrong or unsafe confusion.

OM does **not** declare one universal numeric pass threshold in this package. ISO methods are designed around comprehensibility measurement and standards-specific acceptance criteria; product UI risk varies by action. OM therefore uses a stricter qualitative rule:

- common low-risk actions may remain icon-only only when testing shows strong, stable comprehension in context;
- novel concepts retain labels until evidence is strong;
- security/recovery/destructive meanings keep text even when the icon tests well if text materially reduces risk.

## Deliverables

- `prototypes/icon-comprehension-v1/index.html` — local-only open-answer fixture;
- `contracts/om-icon-comprehension-r12.json` — exact R12 item set and scoring metadata.
