# Local Adaptation and Preference Privacy

**R15 architecture reconciliation:** This detailed design is subordinate to the canonical architecture and ADR-0159. Where earlier R13 wording conflicts with canonical V1 scope/authority, the reconciled wording in this file and [`docs/architecture/ux/RECONCILED_INTERACTION_MODEL.md`](../../architecture/ux/RECONCILED_INTERACTION_MODEL.md) wins.

**Status:** Normative privacy and personalization direction

## 1. Principle

OM may learn how an individual prefers to use the interface, but the user owns both the learned state and the evidence used to produce it.

> **Personalization is local by default, optional, inspectable, resettable, and never a prerequisite for a fast UX.**

## 2. Default privacy boundary

Usage/preference learning data must not be uploaded to an OM service, provider, analytics service, advertiser, model vendor, or other party by default.

No one receives the user's preference or usage history merely because learning is enabled locally.

Sharing, synchronization, research contribution, or diagnostic export requires an explicit user action and a clearly described scope.

## 3. Learning controls

OwnMessenger should expose at least:

- **Learning on/off** — whether new behavioral evidence updates suggestions;
- **Reset learned suggestions** — erase learned ranking state;
- **Manage pins** — explicit choices independent of learning;
- **Optional cross-device preference sync** — if supported, user-controlled and end-to-end protected so infrastructure need not read the preference model;
- **Optional diagnostics/research sharing** — separately consented, never bundled with ordinary personalization.

Turning learning off must leave the Omnibar fully functional using deterministic search, explicit pins, and non-personalized defaults.

## 4. Minimize retained evidence

The personalization engine should prefer derived local features over indefinite raw behavioral logs.

Useful features may include:

- recency;
- frequency;
- time-of-day or coarse context if enabled;
- action success/cancellation;
- explicit pinning;
- repeated query-to-action choices;
- conversation/action affinity.

Do not retain message contents merely to rank UI actions when aggregate local features are sufficient.

If raw event history is used, it should be bounded and locally erasable.

## 5. Ranking algorithm is replaceable

The constitution does not require Bayesian prediction or any specific machine-learning method.

The ranking engine should be pluggable and judged by:

- latency;
- useful top-result accuracy;
- data minimization;
- explainability/debuggability;
- robustness with little history;
- offline operation;
- resistance to pathological feedback loops;
- ability to honor explicit pins and user corrections.

A sensible v1 can begin with transparent recency/frequency/context scoring and optionally evaluate Bayesian updating, contextual bandits, or another proven local-ranking method only if measurements show a real improvement.

Complex ML is not a design goal.

## 6. Explicit preference beats inference

A user's pin, reorder, hide, block, or explicit preference outranks a learned guess.

When the user repeatedly rejects a suggestion, the model should adapt rather than repeatedly asserting the same prediction.

## 7. Context without surveillance

OM may use local contextual signals when enabled, but should not build a broad behavioral profile unrelated to helping the user operate OM.

The personalization boundary is task assistance, not engagement maximization.

The ranking objective should optimize things such as:

- time to desired action;
- number of corrections;
- successful result selection;
- reduced navigation effort;

not time spent, compulsive return, or advertisement value.

## 8. Security and storage

Learned preference state is user data and should receive the same general local-storage protection expected for other private OM state, subject to the final platform/security architecture.

Architecture mapping: learned aggregates are **IdentityProfile-scoped private local state**, encrypted at rest under OM local-data rules, resettable by the user, and excluded from provider/analytics/model upload by default. Backup/migration/sync is only allowed through explicitly approved private-state mechanisms and must not create cross-profile linkage or conversation authority.

## 9. Suggestions must remain understandable

Prediction should never make the interface feel random.

Stable rules:

- pinned items stay stable;
- recent items remain recognizable;
- learned suggestions occupy bounded slots;
- high-risk actions are not promoted solely because they are predicted;
- security-critical actions never bypass confirmation through personalization.

A fast predictable interface is more valuable than a clever but surprising one.

## 10. Learn target likelihood without learning hidden action substitution

Behavioral adaptation may learn that a particular person, conversation, or destination is more likely for a partial query.

It must not infer a different side-effecting verb for an otherwise stable bare entity query. For example, frequent calls to Maria may increase Maria's rank but do not make bare `Maria` mean `Call Maria`.

Alternate verbs come from explicit query language, explicit controls, or explicit user-created pins. This rule makes learned state safer, easier to reset, and less surprising.
