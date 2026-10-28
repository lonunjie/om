# V-018 participant study kit (security-UX comprehension)

- **Date:** 2026-10-28. **Gate:** V-018 `security_ux_comprehension` (pre-release phase; pre-engineering was waived by ADR-0170; ADR-0170/0172 say it blocks any public or beta reference-client release).
- **Status:** ready to run for the items marked RUNNABLE; the items marked NOT BUILT cannot be run until the flow exists. Nothing here is evidence until real people have done it: no result exists yet.
- **Derived from:** the full `pre_release_criteria` text of V-018 in [`machine/architecture_verification_gates.jsonl`](../../machine/architecture_verification_gates.jsonl), [`SECURITY_UX_STUDY_PROTOCOL.md`](../process/SECURITY_UX_STUDY_PROTOCOL.md) (kept as the base; this kit supersedes its flow table), ADR-0129 rules 3, 5, 7, ADR-0111, R-260, R-265, R-267, [`ACCOUNT_CONTINUITY_AND_CONTROL.md`](../architecture/identity/ACCOUNT_CONTINUITY_AND_CONTROL.md) section 11.
- **Readiness context:** [`2026-10-28-beta-gate-readiness.md`](2026-10-28-beta-gate-readiness.md).

## 1. What the gate requires (criteria text, condensed)

1. Moderated sessions, at least **12 non-technical participants**, including at least **3 aged 60+** and **3 with low digital literacy**, in at least **2 languages**, on non-production prototypes.
2. Flows: onboarding tier (T0/T1), recovery on (where it lives), turning recovery off, recovery veto notice, continuity check, device-link notice, suspend device, contact continuity change, bridge E2EE boundary, full-Room service disclosure.
3. Per flow: at least **80% task success** and a **correct answer to the flow's security-comprehension question**; failures are redesigned and retested.
4. Flows added: counterparty notice for a representing service; Connected services review and revoke; "this service can read all Room content" for a full-member service with a trigger filter (ADR-0129).
5. Theme-compiler test: a glyph-remapping theme font changes no reserved surface (ADR-0129 rule 17). **Not a participant task**; an engineering test, not built (no glyph/remap test in `packages/om-design`).
6. Onboarding from a sponsored invite: the user chooses native or one of several independent app origins and can say who serves their app (R-260).
7. Comprehension: "your inviter can see when you get messages and who else on their home you talk to, and can close this mailbox after 30 days' notice" is understood; the co-located-home disclosure is understood (R-265, R-267).
8. Pre-release additionally: **repeat on the real client, plus assistive-technology users** for accessibility.

Interpretation used by this kit (confirm before round 1): "80% per flow" applies to task success and to comprehension separately; task success counts only unaided completion.

## 2. Flows and what the current client can exercise

Checked by grep of `apps/web/src`, `packages/om-ui`, `packages/om-core-harness` and `crates/om-core/src/flows.rs`.

| ID | Flow | Status | Where it can run today |
|---|---|---|---|
| F01 | Onboarding T0 (from an invite) | **RUNNABLE** | Real client (needs a configured Home) and harness. `packages/om-ui/src/onboarding/*`: welcome, lock device, recovery key, invite, ready |
| F02 | Onboarding T1 (no invite, choose a host from a list) | NOT BUILT | Onboarding lets a user skip the invite; no host list. Settings > Your Home only takes a typed address |
| F03 | Recovery on (where it lives) | **HARNESS ONLY** | Harness "Work" profile. Real Core always reports recovery `pending` (status report), so "on" never shows |
| F04 | Turning recovery off | NOT BUILT | The text for "off" exists; no control or intent turns it off |
| F05 | Recovery veto notice | **HARNESS ONLY** | Harness "Work" profile seeds an incoming recovery `rr-demo`. Real Core answers `StartRecovery`/`VetoRecovery` as unsupported |
| F06 | Continuity check | NOT BUILT | No persistent non-blocking check in the UI (no "continuity" in `om-ui`). Nearest: the recovery screen's "pending" line |
| F07 | Device-link notice | **HARNESS ONLY** | Settings > Devices > "Simulate a new device asking to link" then approve. Real Core: device link unsupported |
| F08 | Suspend device | **HARNESS ONLY** | Harness "Work" profile (Phone, Office desktop, Old tablet). Real client has one device, so nothing to suspend |
| F09 | Contact continuity change | PARTLY | The danger banner component exists (`safety_number_changed`); harness conversations seed no notices and real Core cannot produce it yet. Needs a one-line harness seed |
| F10 | Bridge E2EE boundary | NOT BUILT | Only named in [`COMPONENT_CATALOG.md`](../ux-design/COMPONENT_CATALOG.md) (BridgeBoundaryNotice) |
| F11 | Full-Room service disclosure | NOT BUILT | No service or bot UI |
| F12 | Counterparty notice, representing service | NOT BUILT | No service UI |
| F13 | Connected services review and revoke | NOT BUILT | No such screen (Settings has Devices, Recovery, Your Home) |
| F14 | Full-member service with trigger filter | NOT BUILT | No service UI |
| F15 | Sponsored invite: choose native or independent app origin (R-260) | NOT BUILT | "Home facts" shows origin lines as text (`default_origin`, `independent_origin_available`...); no chooser |
| F16 | Inviter visibility and co-located-home disclosure (R-265/R-267) | PARTLY | Observer line "can see who talks to whom and when" exists; no "30 days' notice" line, no co-resident contacts line, no one-operator disclosure |

Totals: 1 runnable on the real client, 4 on the harness only, 2 partly, 9 not built. Unbuilt flows can be tested with a clickable static mock built from the intended copy, but the gate says "on the real client" for the repeat, so a mock is round-1 evidence only. The harness is a **non-production prototype** (it shows "DEV HARNESS - NOT PRODUCTION"), which is what the criteria allow for round 1.

**Language:** the client has English copy only (`apps/web/src/copy/en.ts`; copy is injected, so a translated module is enough). A second language needs a native-speaker translation of the copy for every flow in the study, plus the consent and cards below.

## 3. Prerequisites (checklist)

- [ ] Builds: harness (`cd /Users/maj/dig/lonunjie/om/apps/web && pnpm build:harness && pnpm preview:harness`, serves on port 4173) and, for F01 on the real client, the signed/dev release preview with a Home configured. Tag every session with the git commit.
- [ ] Missing flows built behind the harness flag, or static mocks, for each item you intend to score.
- [ ] Second-language copy module and translated cards, reviewed by a native speaker not on the translation.
- [ ] Study-owned phones and a laptop, each with a dedicated study account in the platform passkey manager (the lock-device step creates a real passkey in whichever manager the device uses). After each session, delete the passkey and reset the browser profile.
- [ ] Test invite links/QR from a test "friend" identity; paper for the recovery key (shred after).
- [ ] An independent facilitator (not a designer of the flows): the protocol treats it as gate evidence.
- [ ] Two raters; a screen recorder; consent forms; payment.
- [ ] Counsel or an ethics adviser has read the consent text for the jurisdiction (this kit is a draft, not legal advice).

## 4. Recruitment screener

Use by phone or form. Read questions verbatim. Keep only the answers needed for eligibility and quotas.

**Eligibility and quotas (targets for round 1):** 12 or more completed participants; at least 3 aged 60+; at least 3 low digital literacy; at least 2 languages, with at least 4 in each (recommended so no language is a token); no more than 4 high-literacy participants; mix of genders; recruit 16 to cover no-shows.

| # | Ask | Rule |
|---|---|---|
| S1 | "Which age group are you in: under 18, 18-29, 30-39, 40-49, 50-59, 60-69, 70 or older?" | Under 18: thank and close. Record the band only |
| S2 | "Which languages do you speak comfortably? Which would you like for the session?" | Session language must be a study language |
| S3 | "Do you, or does someone you live with, work, study or volunteer in software, IT support, cybersecurity, cryptography, app or product design, or at a phone or messaging company?" | Yes: close politely (not non-technical) |
| S4 | "In the last 12 months, did you take part in a research session about a messaging or security app?" | Yes: close |
| S5 | "Do you know anyone on the OM or SeedLife team?" | Yes: close |
| S6 | "How often do you use a smartphone to message people?" | Weekly or more, else close |
| S7 | Literacy items (below) | Score, assign band |
| S8 | "Do you use a screen reader, magnifier, switch device, voice control or other assistive technology?" | Record yes/no and which; needed for the later assistive-technology round |
| S9 | "Sessions last up to 75 minutes and are recorded on screen and, if you agree, by voice. You get [amount] for your time whether or not you finish tasks. Does that work?" | Yes to proceed |

**Digital-literacy items (S7).** For each: "On your own", "With some help", or "Not at all". One point for "On your own".
1. Install a new app on your phone.
2. Send a photo or file in a message or email.
3. Join a video call.
4. Change a password you forgot.
5. Tell whether a message asking for your bank details is a scam.
6. Move your data to a new phone.

Band: 0-2 = **low**, 3-4 = **mid**, 5-6 = **high**.

**Quota tracker (fill as you go):**

| ID | Age band | Literacy | Language | Assistive tech | Date | Status |
|---|---|---|---|---|---|---|
| P01 | | | | | | |

## 5. Consent text (read aloud and give on paper; translate for each study language)

> **Helping us test a messaging app**
>
> We are testing a messaging app called OM. We want to find out whether the app explains things clearly. **We are testing the app, not you.** There are no wrong answers, and nothing you do or say can be a mistake on your part.
>
> **What happens.** For about 75 minutes you will use the app on a phone or laptop we provide and do small tasks, like finding a setting. I will ask what you think things mean. I will not help you during a task, because we need to see how the app works without help. You may take a break at any time.
>
> **Only test accounts.** Please do not enter any of your own passwords, contacts, messages or personal details. Everything you see is a test account.
>
> **Recording.** We record the screen. With your permission we also record your voice so we can write down what you say. We do not record your face. You can say no to the voice recording and still take part. [ ] I agree to voice recording. [ ] I do not.
>
> **Who sees it.** Only the study team and two people who score the answers. Your name is kept in a separate list and is not on the recordings or the results. We publish only scores and short quotes that cannot identify you.
>
> **How long we keep it.** Recordings are deleted after scoring, and in any case within 90 days. Scores without your name are kept as the test record. Your signed form is kept separately for [period set by counsel].
>
> **Payment.** You receive [amount] for your time. You get it even if you stop early or skip a task.
>
> **Your choices.** You may stop at any time, skip any question, and ask us to delete your recording, without giving a reason and without losing your payment.
>
> **Questions.** [Name, phone, email of facilitator].
>
> I am 18 or older. I have read or heard this and agree to take part.
> Name: ____________  Signature: ____________  Date: ________  Session ID (study team fills in): P__

## 6. Session design

- **Length:** up to 75 minutes with a break after about 40. If a participant tires, finish in a second sitting within 7 days.
- **Order:** onboarding block first for everyone (F01, F02 if built, F15, F16, because it needs a fresh profile). Then rotate three blocks between participants: **A** = F03, F04, F06 (recovery), **B** = F05, F07, F08, F09 (devices and contacts), **C** = F10-F14 (services). Assign participants P01.. to the six block orders in turn.
- **Seating:** the participant holds the device. The note-taker sits where the screen is visible; the designer watches by recording or from another room.
- **Rounds:** after any flow fails the gate, the designer revises it between rounds and the revised flow is retested with **new** participants. Number of fresh participants for a retest is not fixed by the gate; this kit proposes 12 with the same quotas (operator decision).
- **Later round:** repeat on the real client, and add assistive-technology users (recruit using S8). The gate states no number for them; record the count and the technology used.

## 7. Moderator script

Read the quoted text verbatim. Do not explain security ideas unless a card says to. Do not confirm or deny correctness.

**Before the participant arrives.** Check the build id, reset the profile and passkey, open the start screen, start the screen recorder, put the cards in order.

**Welcome (about 5 minutes).**

> "Thank you for coming. I am going to ask you to do some small things with a messaging app and tell me what you think. I did not make this app, so you cannot hurt my feelings. If something is confusing, that is useful for us to know. I cannot help you while you work, but I will always be here. Do you have any questions? [Go through the consent form.] May I start the recording?"

**Think-aloud practice (2 minutes).**

> "As you go, please say out loud what you are looking at and what you are thinking. Let's practise: please find the clock on this phone and tell me what you are thinking as you look for it."

**For each flow.**

1. Set the start state from the card. Hand over the device.
2. Read the **task** exactly as written. Start the timer.
3. Stay silent. Allowed neutral prompts only: "What are you looking for?", "What would you do next?", "Take your time." If asked "Is this right?" or "Should I press this?" answer: "What would you do if I were not here?"
4. Stop the task when (a) the end state is reached, (b) the participant says they are done or stuck, (c) they ask three times for help, or (d) the time cap on the card passes. Mark the result: **S** reached the end state unaided; **P** reached it after one neutral prompt; **F** did not, abandoned, or gave up.
5. Immediately ask the **comprehension question** exactly as written. Do not hint. If the answer is a single word, you may say once: "Can you tell me a bit more?" Write the answer down verbatim.
6. Optional, after the question, one confidence rating: "How sure are you, from 1 not at all to 5 completely?"
7. Do not tell the participant whether they were right. Move on: "Thank you. Next one."

**Wrap-up (5 minutes).**

> "That is everything. Is there any part that was confusing or worried you? Is there anything you expected to see and did not?"

Pay, thank, delete the passkey, reset the profile, shred the paper recovery key, note the session id.

**If a participant becomes upset or says they feel tested.** Pause: "You are doing fine, and this is the app being tested, not you. We can stop whenever you like."

## 8. Task cards

Each card: availability, set-up state, **task** (read aloud), success (observable end state), time cap, **comprehension question** (read aloud), and the answer rubric. A comprehension answer is correct only if it contains every "must" element. "Must" elements come from the documented design; the wording of the questions for F12, F13 and F14 and the F03/F06/F16 rubrics are proposals of this kit and must be confirmed by the design owner before round 1.

### F01 Onboarding T0 (from an invite)

- **Availability:** RUNNABLE (real client, harness).
- **Set-up:** fresh profile; a test invite link or QR from the test "friend" identity ready.
- **Task:** "Your friend sent you this link. Start chatting with your friend." (Hand over the link or QR.)
- **Success:** finishes locking the device and the recovery-key step, accepts the invite, reaches the ready screen. **Cap:** 10 minutes.
- **Question:** "Where are your messages kept right now, and who could see that you got a message?"
- **Must:** (a) held by your friend's side (their host or server) for now, not "by OM" and not "only on my phone"; (b) that holder can see *when* a message arrives, not *what* it says.
- **Common wrong:** "OM stores them", "nobody can see anything", "only my phone has them".

### F02 Onboarding T1 (no invite)

- **Availability:** NOT BUILT (needs a list of independent hosts).
- **Task:** "Get set up without a link."
- **Success:** picks a host from the list and finishes setup. **Cap:** 10 minutes.
- **Question:** "Who runs the place you picked?"
- **Must:** names the community or company shown on screen as the runner. Not "OM" or "the app" (if the tested list includes a SeedLife-run host, "SeedLife" is correct for that host).

### F03 Recovery on (where it lives)

- **Availability:** HARNESS ONLY. Harness "Work" profile: recovery on with passkeys and a recovery key.
- **Task:** "Find out whether you could get your account back if you lost this phone."
- **Success:** opens Settings > Recovery and says "yes", naming at least one place it lives (passkeys or recovery key as shown). **Cap:** 4 minutes.
- **Question:** "If you lost your phone, how would you get back in, and how long would it take?"
- **Must:** (a) a method shown on screen (passkeys or recovery key); (b) a wait about as long as the screen states (a few days, not "instantly").

### F04 Turning recovery off

- **Availability:** NOT BUILT.
- **Task:** "Make this account one that disappears if you lose the phone."
- **Success:** completes the turn-off including its confirmation. **Cap:** 4 minutes.
- **Question:** "What happens if you lose the phone now?"
- **Must:** the account is gone for good and cannot be recovered.

### F05 Recovery veto notice

- **Availability:** HARNESS ONLY. Harness "Work" profile has an incoming recovery request.
- **Tell the participant before:** "You did not start anything like this."
- **Task:** "A notice has appeared on your screen. What do you do?"
- **Success:** opens it ("Review it") and stops it ("Stop it"). **Cap:** 3 minutes.
- **Question:** "What would have happened if you ignored it?"
- **Must:** someone else could have got into or taken over the account (after the waiting time).

### F06 Continuity check

- **Availability:** NOT BUILT (persistent reminder). Nearest harness state: recovery policy `pending`.
- **Set-up:** account where recovery is pending, off, or the only custodian is on the only device.
- **Task:** "OM keeps showing a reminder about your account. Read it and deal with it."
- **Success:** follows the reminder to save a recovery key or set up recovery. **Cap:** 4 minutes.
- **Question:** "What is the reminder warning you about?"
- **Must:** losing this phone could lose the account for good; and a way to prevent it (recovery key or passkey recovery).

### F07 Device-link notice

- **Availability:** HARNESS ONLY. Use "Simulate a new device asking to link", approve it, then ask.
- **Tell the participant before:** "You did not add any new device."
- **Task:** "A notice appeared on your screen. Was this you? Deal with it."
- **Success:** says it was not them, opens the device and removes or suspends it. **Cap:** 4 minutes.
- **Question:** "What can that new device do?"
- **Must:** read your messages and send messages as you (everything your account can do).
- **Variant for warm-up (not scored):** "You just added your own laptop": the right action is to leave it.

### F08 Suspend a device

- **Availability:** HARNESS ONLY. Devices: Phone, Office desktop, Old tablet.
- **Task:** "Your office desktop was stolen. Stop it reading your messages."
- **Success:** opens its details and suspends (or removes) it through the confirmation. **Cap:** 4 minutes.
- **Question:** "Can the office desktop still read new messages?"
- **Must:** no, it no longer gets new messages. (If they add that old messages already on it stay there, that is correct; do not penalize.)

### F09 Contact continuity change

- **Availability:** PARTLY. The banner exists; a harness seed must put it on a conversation with "Maria".
- **Set-up:** conversation with Maria showing "This contact's safety number changed. Check it with them in person or another trusted way before you rely on this chat."
- **Task:** "Maria's security details changed. What now?"
- **Success:** says they would check with Maria another way (call, meet) before relying on the chat; does not just dismiss it. **Cap:** 3 minutes.
- **Question:** "Why might that happen, and when should you worry?"
- **Must:** (a) an innocent cause (new phone, reinstalling, recovering an account); (b) worry if it was unexpected, and check with her.

### F10 Bridge E2EE boundary

- **Availability:** NOT BUILT.
- **Task:** "Send a message to someone on another app using this bridge."
- **Success:** sends via the bridge after seeing its notice. **Cap:** 4 minutes.
- **Question:** "Who could read this message?"
- **Must:** the bridge (its operator) can read it, in addition to the recipient. Wrong: "only me and the person I wrote to".

### F11 Full-Room service disclosure

- **Availability:** NOT BUILT.
- **Task:** "A bot joined this group. Check what it can see."
- **Success:** finds the member list or notice describing the bot's access. **Cap:** 3 minutes.
- **Question:** "Can the bot read everything here?"
- **Must:** yes, it is a full member and can read all messages in the group.

### F12 Counterparty notice for a representing service

- **Availability:** NOT BUILT. Notice per ADR-0129 rule 5: "Alice's messages here are handled by Assistant".
- **Task:** "You are about to write to Alice. A notice appears. Tell me what it means and decide whether to go ahead."
- **Success:** reads the notice, finds both "continue" and "decline", and gives a reason for the choice (either choice is acceptable). **Cap:** 4 minutes.
- **Question:** "Besides Alice, who will receive what you write here?"
- **Must:** the Assistant service, and that it can read it.

### F13 Connected services: review and revoke

- **Availability:** NOT BUILT. Connected services sits next to Devices (ADR-0129 rule 7).
- **Set-up:** account with one connected service named "Assistant".
- **Task:** "Find out which services are connected to your account, and disconnect Assistant."
- **Success:** opens Connected services and disconnects it with one deliberate confirmation. **Cap:** 4 minutes.
- **Question:** "While Assistant was connected, what could it see?"
- **Must:** the messages it was handling for the account (not "nothing" and not "only my name").

### F14 Full-member service with a trigger filter

- **Availability:** NOT BUILT. Service "Summarizer" with a trigger filter (acts only on messages starting "/summarize").
- **Task:** "Look at how this service is set up and tell me what it can see."
- **Success:** opens its details. **Cap:** 3 minutes.
- **Question:** "Does it see only the messages that ask it to do something, or everything in the group?"
- **Must:** everything in the group; the filter only limits when it acts, not what it can read.

### F15 Sponsored invite: choose where the app comes from (R-260)

- **Availability:** NOT BUILT (information text only today).
- **Set-up:** invite from a sponsor; the screen offers a phone/computer app and at least two shared web addresses run by different, independent operators.
- **Task:** "Your friend's invite gives you a few ways to open OM. Choose one."
- **Success:** chooses an option unprompted and completes the step; the choice is not the sponsor's own address unless the participant explicitly opts in. **Cap:** 4 minutes.
- **Question:** "Who provides the app you are using now?"
- **Must:** names the runner of the chosen option as shown on screen. Not "my friend" unless they chose the sponsor's address knowingly.

### F16 Inviter visibility and co-located-home disclosure (R-265, R-267)

- **Availability:** PARTLY (one observer line today; the full first-run disclosure is not built).
- **Task:** "Read this screen about the mailbox your friend gave you. Then tell me in your own words what your friend can and cannot see or do."
- **Success:** reads the screen to the end. **Cap:** 4 minutes.
- **Question A:** "What can your inviter see, and can they stop your mailbox?"
  - **Must (all three):** they can see when your messages arrive; they can see who else on their home you talk to; they can close the mailbox after 30 days' notice.
  - Bonus, not required: they cannot read message content.
- **Question B:** "The same host looks after your mailbox, your introductions and your directory entry. What does that mean for you?"
  - **Must:** one party can see all of those and connect them (who you talk to and when); it is not assumed they stay separate. Not about message content.
- **Comprehension is correct only if A and B are both correct.**

## 9. Scoring

**Per participant per flow:** task result S / P / F; time in seconds; neutral prompts used; error code(s); comprehension answer verbatim; confidence 1-5.

**Error codes:** E1 did not notice the notice or control; E2 misread the wording; E3 chose a wrong control; E4 gave up; E5 stuck on vocabulary (write the word); E6 hesitated from distrust; E7 other.

**Comprehension scoring.** Two raters score each answer against the "Must" list independently from the transcript, 1 = every Must element present, 0 = otherwise. Disagreements are resolved by discussion to a final score; record both raw scores. Extra wrong statements alongside a correct core do not fail an answer unless they contradict a Must element.

**Pass rule per flow:** observed task success (S only) at least 80% **and** observed comprehension at least 80%, with the cohort quotas met. Minimum counts that reach 80%:

| n scored | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 |
|---|---|---|---|---|---|---|---|---|---|
| successes needed | 10 | 11 | 12 | 12 | 13 | 14 | 15 | 16 | 16 |
| Wilson 95% interval at that count | 55-95% | 58-96% | 60-96% | 55-93% | 57-93% | 59-94% | 61-94% | 62-94% | 58-92% |

Report the interval beside every rate: at n=12, passing at 10 of 12 is consistent with a true rate as low as about 55%. Also report each flow by cohort (60+, low literacy, each language) and give every failing flow a redesign ticket with the error codes.

**Score sheet columns** (one row per participant per flow; save as `results.csv`):

`participant,language,age_band,literacy,flow,task,comp_final` (age_band: 18-39, 40-59, 60+; literacy: low, mid, high; task: S, P, F; comp_final: 1 or 0), plus free columns for time, prompts, error codes, raw rater scores and quote.

**Scoring script** (standard library only; verified on a synthetic file). Save as `v018_score.py` and run `python3 -I v018_score.py results.csv`:

```python
#!/usr/bin/env python3
"""V-018 scoring. CSV header: participant,language,age_band,literacy,flow,task,comp_final
Only task == S counts as success. comp_final is 1 or 0 after rater adjudication."""
import csv, math, sys
def wilson(k, n, z=1.96):
    if n == 0: return (0.0, 0.0)
    p = k / n; d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d; h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return (max(0.0, c - h), min(1.0, c + h))
rows = list(csv.DictReader(open(sys.argv[1], newline="", encoding="utf-8")))
people = {}
for r in rows: people[r["participant"]] = r
n_all = len(people)
n60 = sum(1 for r in people.values() if r["age_band"] == "60+")
nlow = sum(1 for r in people.values() if r["literacy"] == "low")
langs = {r["language"] for r in people.values()}
cohort_ok = n_all >= 12 and n60 >= 3 and nlow >= 3 and len(langs) >= 2
print(f"participants={n_all} aged60+={n60} low_literacy={nlow} languages={sorted(langs)} cohort_ok={cohort_ok}")
print(f"{'flow':<6}{'n':>3} {'task':>8} {'95% CI':>9} {'comp':>8} {'95% CI':>9}  verdict")
for f in sorted({r["flow"] for r in rows}):
    fr = [r for r in rows if r["flow"] == f]; n = len(fr)
    t = sum(1 for r in fr if r["task"] == "S"); c = sum(1 for r in fr if r["comp_final"] == "1")
    tl, th = wilson(t, n); cl, ch = wilson(c, n)
    ok = n >= 12 and t / n >= 0.8 and c / n >= 0.8 and cohort_ok
    ti, ci = f"{100*tl:.0f}-{100*th:.0f}%", f"{100*cl:.0f}-{100*ch:.0f}%"
    print(f"{f:<6}{n:>3} {t:>2}/{n:<2} {100*t/n:>3.0f}% {ti:>9} {c:>2}/{n:<2} {100*c/n:>3.0f}% {ci:>9}  {'PASS' if ok else 'FAIL/INCOMPLETE'}")
```

## 10. Data-handling rules

1. **Test data only.** No real accounts, contacts, messages or passkeys. Study accounts and devices are dedicated; passkeys created during a session are deleted after it, the browser profile is reset, paper recovery keys are shredded.
2. **Pseudonymous ids.** Participants are P01, P02, ... The name-to-id list, consent forms and payment records are kept apart from recordings and results, in a different place with different access.
3. **Minimum collection.** Screener answers other than age band, language, literacy band and assistive-technology use are not kept after eligibility is decided.
4. **Recordings.** Screen (and voice only with consent), never the face. Stored encrypted by the facilitator's organisation, access limited to the facilitator, note-taker and raters. Deleted after scoring and within 90 days. **Do not store raw recordings, consent forms or the id list under `~/dig` or any Resilio-synced folder**: that tree replicates to another host.
5. **Publication.** Only anonymised scores (`verification/results/v018-study-round-<n>.json`, no names, no dates finer than the week, no free-text that could identify someone) and short quotes that cannot identify the speaker.
6. **Rights.** Participants may stop, skip, or have their recording deleted at any time, without losing payment.
7. **Agents.** An AI agent must never receive recordings, names or the id list; it may be given the anonymised score file.
8. **Legal.** Retention periods, consent wording and any ethics or data-protection filing depend on where the study runs; counsel confirms them (feeds PR-001).

## 11. Results template

Write one report per round, signed by the independent facilitator, plus the anonymised JSON beside it.

```markdown
# V-018 study round <n> report

- Round: <n>   Dates (week of): <date>   Build: <harness|real client> @ <git commit>   Languages: <list>
- Facilitator: <name, organisation>   Relationship to the flow designers: <none | describe>   Raters: <2 names/ids>
- Participants: total <n>; aged 60+ <n>; low literacy <n>; mid <n>; high <n>; per language <...>; assistive technology <n, kinds>
- Deviations from the kit: <list, or none>

## Per-flow results
| Flow | Built? | n | Task success (S) | 95% CI | Comprehension | 95% CI | Pass? | Main errors |
|---|---|---|---|---|---|---|---|---|
| F01 | real | 12 | 10/12 83% | 55-95% | 11/12 92% | 65-99% | yes | E2 x1 |

## Subgroups (60+, low literacy, each language)
<table or "no flow differs by more than 20 points">

## Failures and redesign tickets
| Flow | Evidence (codes, quotes) | Ticket | Retest round |

## Flows not run
<id, reason: not built | out of time>

## Threats to validity
<small n, prototype fidelity, translation, learning effects, facilitator effects>

## Gate statement
Facilitator signature and date. Statement: "The flows listed as passed met both thresholds with the stated cohort. Flows not run or failed remain open."
```

```json
{
  "kit": "v018-participant-study-kit/1",
  "round": 1,
  "week_of": "2026-11-02",
  "build": {"kind": "harness", "commit": "<sha>"},
  "cohort": {"total": 12, "age_60_plus": 4, "low_literacy": 4, "languages": {"en": 6, "<other>": 6}, "assistive_technology": 0},
  "flows": [
    {"id": "F01", "n": 12, "task_success": 10, "comprehension_correct": 11, "pass": true, "error_codes": {"E2": 1}}
  ],
  "flows_not_run": [{"id": "F02", "reason": "not built"}],
  "facilitator_independent": true
}
```

Update the V-018 record (`criteria_met`, `progress`) and [`SNAPSHOT.md`](../../SNAPSHOT.md) only after the independent report exists, through the normal checkpoint process.

## 12. Differences from `SECURITY_UX_STUDY_PROTOCOL.md`

The protocol's table lists 10 flows. This kit adds the criteria items the table omits (continuity check, the three service flows, the R-260 origin choice, the R-265/R-267 comprehension), splits "recovery on" and "turning recovery off", keeps the protocol's rubric where flows overlap, and replaces the "Maria / laptop" wording with the harness's seeded names. If the operator adopts this kit, update the protocol to point here.
