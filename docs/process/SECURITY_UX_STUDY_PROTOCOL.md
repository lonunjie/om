# Security UX Comprehension Study Protocol (V-018)

- **Status:** Study kit, ready to run. Running it needs people, which is the V-018 pre-engineering criterion itself:
  at least 12 non-technical participants, including at least 3 aged 60+ and 3 with low digital literacy, in at least
  2 languages, with an independent facilitator.
- **Prototype:** `docs/ux-design/prototype/` (static screens), extended with the flows below before the study.

## Roles

- **Facilitator:** not the designer of the flows being tested. Reads the scripts verbatim and never explains a
  security concept unless the script says so.
- **Note-taker:** records task success, errors, time and verbatim quotes.
- **Designer:** watches silently or from a recording; changes flows only between rounds.

## Participants and consent

Recruit through community partners (T1 hosts are good candidates; see ADR-0114). Use plain-language consent in the
participant's language. Record the screen only, not the face, unless the participant opts in. Pay for time.
Participants may stop at any point. Use no real personal data; test accounts only.

## Flows, tasks and comprehension questions

Pass rule per flow: at least 80% task success **and** at least 80% correct answers to its comprehension question. A
flow that fails is redesigned and retested with new participants.

| # | Flow | Task (read aloud) | Comprehension question (open, then scored) | Correct if the answer says… |
|---|---|---|---|---|
| 1 | Onboarding from an invite (T0) | "Your friend sent you this link. Start chatting with her." | "Where are your messages kept right now, and who could see that you got a message?" | the friend's home holds them for now; she can see *when*, not *what* |
| 2 | Onboarding without an invite (T1 list) | "Get set up without a link." | "Who runs the place you picked?" | a community or host, not OM |
| 3 | Recovery is on | "Find out whether you could get your account back if you lost this phone." | "If you lost your phone, how would you get back in, and how long would it take?" | passkeys or recovery key; a few days' wait |
| 4 | Turning recovery off | "Make this account one that disappears if you lose the phone." | "What happens if you lose the phone now?" | the account is gone for good |
| 5 | Recovery veto notice | (a notice appears) "What do you do?" | "What would have happened if you ignored it?" | someone else could take the account after the wait |
| 6 | Device-link notice | (a new-device notice appears) "Was this you? Deal with it." | "What can that new device do?" | read and send as you |
| 7 | Suspend a device | "Your laptop was stolen. Stop it reading your messages." | "Can the laptop still read new messages?" | no |
| 8 | Contact continuity change | "Maria's security details changed. What now?" | "Why might that happen, and when should you worry?" | new phone or recovery; worry if unexpected; check with her |
| 9 | Bridge boundary | "Send a message to someone on another app via this bridge." | "Who could read this message?" | the bridge operator can |
| 10 | Full-Room service disclosure | "A bot joined this group. Check what it can see." | "Can the bot read everything here?" | yes, if it has full-Room access |

## Scoring and analysis

Two raters score answers independently against the rubric and resolve disagreements. Report each flow's success and
comprehension with a 95% interval (at n=12, 80% observed has a wide interval; report it honestly). Cluster the errors,
and give every failing flow a redesign ticket.

## Deliverable

`verification/results/v018-study-round-<n>.json` (anonymized scores), plus a short report signed by the facilitator.
The report and the independence of the facilitator are the gate evidence.
