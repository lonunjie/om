# Relay setup walkthrough: a fresh Cloudflare and GitHub account

- **Date:** 2026-11-14. **Status:** a script for the operator to follow; nothing here has been run yet. Everything about what Cloudflare and GitHub show is **unverified** until you record it.
- **Why:** the "Set up my own, free" flow stalled when the person's Cloudflare GitHub app allowed only selected repositories ([`2026-11-04-handoff-next-week.md`](2026-11-04-handoff-next-week.md) section 1). The app now asks for both accounts first (deviation D35 in [`2026-10-28-relay-status.md`](2026-10-28-relay-status.md)). This walkthrough checks whether that is enough and what each screen really says.

## What to record at every screen

For each numbered screen below write, in a scratch note (screenshots stay in the session scratchpad, never in the repository): the page address, the heading, the exact button labels, anything that asks for permission, and what happened after you pressed the button (a new page, an error, nothing). Copy error text exactly. Note the time, so a slow page is not mistaken for a stuck one.

## Setup

1. Use a clean browser profile (no saved Cloudflare or GitHub sign-ins). Open the OM web app, press **Get started**, then **Set up my own, free**.
2. Record: the **Before you start** page. Check that both **I have it** buttons start unpressed, **Continue** is off until both are pressed, and pressing a button again takes the confirmation back.

## Path A: GitHub app allowed for selected repositories only

3. Make a new free Cloudflare account (cloudflare.com). Record every screen of the sign-up, including any email check or card prompt. **A card prompt would break the "free, no card" claim: stop and write it down.**
4. Make a new free GitHub account. Create **one empty repository** in it (any name), so "selected" has something to select.
5. Back in OM, press **I have it** twice and **Continue**. Copy the setup value. Press **Open Cloudflare setup**.
6. On the Cloudflare page: record whether it asks to connect GitHub, and the exact permission screen on GitHub. When GitHub asks which repositories Cloudflare may use, choose **Only select repositories** and select only the empty repository.
7. Record what happens next: does Cloudflare create the new repository, show an error, stall, or send you somewhere else? If it stalls, press **Open Cloudflare setup** in OM again and record whether it resumes.
8. If you reach the form: record the label of the setup value box (**OM setup value**, **OM_ADMISSION**, or something else) and whether the long description from the template shows anywhere. Paste the value, deploy, and record the build log's last lines and the final address. Type the address in OM and record the result.

## Path B: all repositories

9. In GitHub settings, Applications, change the Cloudflare app's access to **All repositories** (or start again with a second new GitHub account). Repeat steps 5 to 8 and record the differences from path A.

## Path C: a new account made only for this (the advice we give)

10. Use a brand-new GitHub account with no repositories at all, and choose **All repositories** when asked. Repeat steps 5 to 8. This is the path the app recommends to anyone who does not want to share their own repositories, so it matters most.

## Questions to answer from the record

- Does path A stall, and does reopening the setup link resume it? (If not, the "selected repositories" advice needs a manual path: handoff section 1, alternatives a to c.)
- Do paths B and C finish without any step the app has not told the person about?
- Is a card or phone number asked for anywhere?
- Which label does the value box carry? Does the app's wording ("the box named OM setup value; Cloudflare may show OM_ADMISSION") match?
- Does GitLab appear as a choice at all? We claim nothing about it until you have tried it.
- Was **Before you start** too much? If both paths finish without it, the pre-steps are friction that prevents no stall: say so, and they can be shortened or removed.

## After the run

Record the findings in the relay status report (D35, "What is left"), update `onboardingCopy.relay.cloudflare` to match what was seen, and, if the box description did not show, shorten the template description so it begins with "OM setup value" (a new template release).
