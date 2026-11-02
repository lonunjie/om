// OPT-IN (OM_LIVE_RELAY=1): the same two-person flow against the LIVE Home with the production build. Two fresh identities
// invite, approve and message through the deployed service; what it leaves behind is a descriptor and key bundles that the
// Home expires by its own TTLs. Never part of the hermetic run.
import { test } from "@playwright/test";
import { inviteAndChat } from "./invite-flow.ts";

test.skip(process.env["OM_LIVE_RELAY"] !== "1", "opt-in: set OM_LIVE_RELAY=1 (this talks to the live Home)");
const BASE = process.env["OM_INVITE_BASE"] ?? "http://localhost:4178"; // OM_INVITE_BASE: a deployed preview of the signed bundle
test.use({ baseURL: BASE });

test("two people message through the LIVE Home from a shared link", async ({ browser }) => { await inviteAndChat(browser, BASE); });
