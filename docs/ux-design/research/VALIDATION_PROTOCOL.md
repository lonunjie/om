# OM UI/UX Validation Protocol

This protocol converts the UI/UX constitution into repeatable tests. It deliberately separates **automated conformance**, **instrumented performance**, and **human usability** because none substitutes for the others.

## A. Automated checks

Run on every default/preset theme and representative screen state:

1. text contrast;
2. non-text/UI contrast;
3. focus visibility geometry/contrast;
4. required accessible labels/roles/names;
5. keyboard reachability of interactive elements;
6. no color-only critical state;
7. 200% text-scale/reflow screenshots;
8. Reduce Motion behavior;
9. minimum hit-target policy for OM-owned high-frequency controls;
10. light/dark semantic-token completeness.

The included `validation/run_static_checks.py` covers only the token/contrast subset. Browser accessibility tooling and human review remain required.

## B. Instrumented task benchmark

Use fixture datasets of at least:

- 100 people;
- 1,000 conversations;
- 100,000 messages;
- 1,000,000 messages for stress testing.

Measure:

- Omnibar keystroke-to-paint;
- candidate-generation latency;
- first useful result;
- first full-text result batch;
- result-list movement after selection begins;
- chat-list scroll/frame stability;
- composer input latency;
- memory/storage growth of local indexes.

Report median and p95, plus device/browser details. Do not optimize only on flagship hardware.

## C. Core human tasks

### T1 — zero-contact first run

Prompt: "You just installed OM and want to start chatting with a family member who is not yet connected."

Success: participant finds invite/QR path without instruction.

Observe: hesitation, accidental permission-seeking, whether privacy posture is understood without a lecture.

### T2 — open and reply

Prompt: "Open Maria's latest message and reply to it."

Success: open conversation, select/gesture reply, compose, send.

Observe: discoverability and whether gesture alternatives are visible.

### T3 — Omnibar person/action distinction

Prompt: "Message Maria. Now call Maria."

Success: bare Maria reliably opens/messages; call requires an explicit verb/action.

Observe: whether prediction creates surprise.

### T4 — recover from failure

Prompt: fixture marks a message failed.

Success: participant notices failure, understands which message failed, and retries without searching global settings.

### T5 — find old content

Prompt: "Find the message where Ren sent the hotel address."

Success: use Omnibar/local search and open the correct context.

### T6 — keyboard-only

Repeat T2/T3/T5 without pointer.

### T7 — large text

Repeat T1/T2 at 200% text scaling.

### T8 — reduced motion

Repeat navigation and Omnibar morph with reduced motion enabled. Meaning should remain clear.

### T9 — one-handed mobile

Repeat common actions with each hand. Record reach failures/mis-taps; do not infer handedness silently.

### T10 — theme semantic parity

Repeat failure/warning/verified/focus identification in OM Clear Light and Dark plus at least one alternate preset. Meaning must transfer.

## D. Suggested measurements

Record:

- task completion;
- critical/noncritical errors;
- time on task;
- first-action latency;
- backtracks/reversals;
- mis-taps;
- help requests;
- perceived ease and confidence;
- short workload measure where useful;
- qualitative "what did you expect to happen?" notes at hesitation points.

Avoid overfitting to one scalar score. OM's goal is natural, trustworthy interaction, so expectation violations and recovery failures matter even when completion time is fast.

## E. Evidence base used for the gates

- WCAG 2.2 — use of color, contrast, resize/reflow, focus, dragging, target size, and concurrent input: https://www.w3.org/TR/wcag/
- W3C Focus Appearance explanation: https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance
- Apple HIG Accessibility — Reduce Motion and simplified/low-cognitive-load presentation guidance: https://developer.apple.com/design/human-interface-guidelines/accessibility
- Apple HIG Motion — motion for status/feedback/instruction and adaptation to input/accessibility: https://developer.apple.com/design/human-interface-guidelines/motion
- Nielsen Norman Group, Progressive Disclosure — frequent actions belong up front; advanced actions remain clearly discoverable: https://www.nngroup.com/articles/progressive-disclosure/
- Display polarity, color-search, one-handed touch, and aesthetics/usability studies are catalogued in [`VISUAL_ERGONOMICS_EVIDENCE.md`](VISUAL_ERGONOMICS_EVIDENCE.md).

## F. What this package has and has not validated

Revision 10 additionally supplies an executable PWA conformance harness. In the available Chromium runtime, 17/17 executable DOM/layout/input checks passed. Firefox and WebKit were not runnable in the environment and are recorded as `not_run`. This still **does not claim real-user testing, assistive-technology interoperability, installed-PWA lifecycle validation, or physical-device ergonomics have been completed**. Those results must be recorded separately when executed.
