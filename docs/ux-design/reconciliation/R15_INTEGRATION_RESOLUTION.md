# R15 — Architecture ↔ UI/UX Integration Resolution

**Base architecture:** `om-design-20260929-0634-PHT-reviewed-fixed.zip` (`78ddae29…bbe99`)  
**Base UI/UX:** R14 (`73ba744c…14a37`)  
**Decision:** ADR-0159  
**Protocol/Core redesign required:** **No**

## Result

R15 integrates the standalone UI/UX design track into the canonical architecture rather than carrying two competing UX trees.

### Integrated architecture changes

- Chats-only default compact shell; additional destinations user-composable.
- Omnibar becomes the universal local-first find/do surface.
- semantic Action Registry is explicitly subordinate to typed Core intents/PlatformAdapter capabilities.
- local learning is IdentityProfile-scoped, private/resettable and cannot learn hidden side-effecting verbs.
- old fixed three-tab screen map is replaced.
- full architecture V1 surface is retained under progressive disclosure: scheduled send, draft sync, Spaces/nested Spaces/Rooms, threads/Topics, Publication, polls, stickers/bounded loops, rich text, disappearing/view-once, ContactCapsule/MessageRequest, link previews and selective report/evidence.
- T0–T4 onboarding and sponsored client-origin choice are integrated with invite-first first run.
- recovery-on-by-default, Protection profiles, Suspend/quarantine, pending/final revocation and Safety Freeze are integrated.
- call timeline wording no longer implies call signaling is a durable ConversationEvent.
- OwnMessenger/OhMessenger remain presentation-only safety-bounded layers.
- detailed specs/contracts/tokens/assets/prototypes/validators move under `docs/ux-design/`.

### Preserved open evidence gates

- **V-018** security UX comprehension remains open. Human/multilingual/nontechnical evidence was deliberately deferred.
- **V-022** performance budgets remains open. Prototype/browser component targets do not replace the required reviewed plan/real-device evidence.

### Deferred human/brand evidence

Final identity mark/wordmark, novel icon comprehension, physical ergonomics and final brand freeze remain evidence-gated. They do not block semantic architecture integration.

## Authority rule

When a detailed `docs/ux-design/` artifact and canonical architecture disagree, this precedence applies:

1. constitution / accepted ADRs / machine truth;
2. canonical `docs/architecture/` domain docs;
3. `docs/ux-design/` design references;
4. prototypes/research evidence.

The integrated UX tree therefore cannot silently amend recovery, group, publication, event, provider or cryptographic authority.
