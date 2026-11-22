# Real relay setup test

Allow **55 minutes**. This test uses the real Cloudflare and GitHub screens. The offline Playwright rehearsal uses synthetic screens and does not replace this run.

Use the OM build under test and dedicated free test accounts. Start in a fresh browser profile. After a path connects, start the next path in another fresh profile so it gets its own setup value. Keep each OM tab open until its path ends. Record times, headings, button labels, permission choices and redacted errors in a local note. Keep sign-in details, codes and private links out of the record.

| Time | Run | Record |
|---|---|---|
| 0–10 min | In OM, choose **Get started → Set up my own, free**. Check that neither account is confirmed and that **Continue** stays disabled until both are confirmed. Toggle one back off. Create or sign in to the Cloudflare test account and a GitHub test account with one empty repository. | App build, email or phone checks, any payment prompt and whether the confirmations helped. Stop the affected path if it asks for payment details. |
| 10–20 min | Confirm both accounts and copy the OM setup value. Open Cloudflare setup. If GitHub offers **Only select repositories**, choose just the empty test repository. If setup stalls or leaves you on GitHub, open the OM setup link once more. Run the completion check if the form is reachable. | Permission wording, which repositories were visible, and where both attempts ended. Did reopening resume, restart or stall? |
| 20–30 min | On that test account, try **All repositories** if offered. If the previous attempt already connected, use another test deployment. Run the completion check below if the form is reachable. | What changed from selected access and any extra steps. |
| 30–45 min | Repeat in a fresh profile with a new GitHub test account that has no repositories. Confirm the permission screen names the new account, then choose **All repositories** if offered. Run the completion check. | Whether the new account can complete setup and whether OM's instructions match each step. |
| 45–50 min | Look for a GitLab choice. If it appears and a test GitLab account is ready, try it without a GitHub sign-in. If it is absent, return to OM and record where a person without GitHub is left. | Offered, tried, connected or blocked. |
| 50–55 min | Summarize each path as connected, blocked or untried, with the last screen and next action. Record any test resources created. | Copy changes supported by what you saw and questions that still need testing. |

**Completion check.** Record the setup field's exact label and whether help text appears. Paste the value from the matching OM tab, deploy, and record the final status. For a quota or deployment error, record the provider's error and what it offers next. After success, copy the service address into OM and select **Check and continue**. Finish first run, choose **Show my QR**, and reload to confirm that OM still uses the service. Record where the check ended.

Move to the next path when its time is up. Use the [original walkthrough](../reports/2026-11-14-relay-setup-walkthrough.md) for any follow-up.
