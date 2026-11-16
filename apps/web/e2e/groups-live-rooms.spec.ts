// Groups on the REAL Core with the OpenMLS engine module, over the REAL carriage on the local reference Home (ADR-0189), with every person in
// a browser context of their own: a Room created with Hardened, tightened and loosened (the explicit confirmation, and the "Less strict"
// notice the other member sees, in the contact's name); a Room of three people joined on the heads the Welcome carries (the two newcomers
// have never met), then a removal and the route update that follows; and an invitation held on a device that has not got the groups module
// yet, which survives a reload and then joins. Only what a person can do on screen is driven. The Home only ever sees sealed objects.
import { expect, test } from "@playwright/test";
import {
  closeAll, createGroup, expectSealedOnly, firstLink, hears, joinByLink, joinGroup, meet, nextLink, openGroup, readyGroups, roomLog, say, startPerson, type Person,
} from "./groups-live-people.ts";
import { reopen } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4180" });
test.setTimeout(240_000);

const everyone: Person[] = [];
test.afterEach(async () => { await closeAll(everyone.splice(0)); });

test("a Hardened Room: created with the level, tightened at once, loosened only after a confirmation, and the other member sees who made it less strict", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const link = await firstLink(alice);
  const bob = await joinByLink(browser, "Bob", link, alice);
  everyone.push(bob);
  await readyGroups(bob); // Bob has used groups before: the invitation is an ordinary card (the held path has its own test)

  await createGroup(alice, "Planning", ["Bob"], "Hardened");
  await joinGroup(bob, "Planning", "Alice");
  await openGroup(alice, "Planning");
  await say(alice, "first words");
  await hears(bob, "first words");

  // Created Hardened: only admins can add people, and an admin approves each new person. Maximum Privacy is the stricter level.
  await alice.page.getByRole("button", { name: /^People in Planning/ }).click();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Room settings" }).click();
  const sheet = alice.page.getByRole("dialog", { name: "Settings for Planning" });
  await expect(sheet.getByText("Only admins can add people.")).toBeVisible();
  await expect(sheet.getByText("Yes. An admin approves each new person.")).toBeVisible();
  await expect(sheet.locator("[data-current=true]")).toContainText("Hardened");

  // Tighten: applies at once, no question.
  await sheet.getByRole("button", { name: "Use Maximum Privacy" }).click();
  await expect(sheet.locator("[data-current=true]")).toContainText("Maximum Privacy");

  // Loosen: asks first and names what changes; "Keep it as it is" changes nothing.
  await sheet.getByRole("button", { name: "Use Recommended" }).click();
  const ask = sheet.getByRole("group", { name: "Make this group less strict?" });
  await expect(ask.getByText("Anyone in the group will be able to add people.")).toBeVisible();
  await expect(ask.getByText("Everyone in the group will see this change and who made it.")).toBeVisible();
  await ask.getByRole("button", { name: "Keep it as it is" }).click();
  await expect(sheet.locator("[data-current=true]")).toContainText("Maximum Privacy");
  await sheet.getByRole("button", { name: "Use Recommended" }).click();
  await sheet.getByRole("group", { name: "Make this group less strict?" }).getByRole("button", { name: "Make less strict" }).click();
  await expect(sheet.locator("[data-current=true]")).toContainText("Recommended");
  await sheet.getByRole("button", { name: "Close" }).click();

  // Alice's own log says what she did; Bob, on his own device, sees that Alice made the Room less strict, in the name his contact list gives her.
  await expect(roomLog(alice).getByText(/^You changed the protection level: now Recommended\.$/)).toBeVisible();
  await expect(roomLog(alice).getByText("Less strict").first()).toBeVisible();
  const line = roomLog(bob).locator(".om-group-notice__line").filter({ hasText: "Alice changed the protection level: now Recommended." });
  await expect(line).toBeVisible({ timeout: 60_000 });
  await expect(line.getByText("Less strict")).toBeVisible();
  // The tightening to Maximum Privacy was announced too, and Bob can still talk.
  await expect(roomLog(bob).getByText("Alice changed the protection level: now Maximum Privacy.")).toBeVisible();
  await say(bob, "seen it");
  await hears(alice, "seen it");
  expectSealedOnly(alice, [/first words|seen it|Planning/]);
  expectSealedOnly(bob, [/first words|seen it|Planning/]);
});

// Pins today's behaviour (D93): the Add Commit carries no account head, so the member who does not know the newcomer holds it.
// When D93's decided fix lands (the Commit frame carries the added accounts' signed heads like the Welcome), this test flips.
test("three people who have never all met: the newcomer joins on the heads the Welcome carries and talks with the inviter; the member who does not know her waits, and says so", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  const carol = await joinByLink(browser, "Carol", await nextLink(alice), alice); // Carol and Bob have never met
  everyone.push(carol);
  await readyGroups(bob);
  await readyGroups(carol);

  await createGroup(alice, "Trio", ["Bob"]);
  await joinGroup(bob, "Trio", "Alice");
  await openGroup(alice, "Trio");
  await say(alice, "hello bob");
  await hears(bob, "hello bob");

  // Alice adds Carol to the running Room: the Welcome names Bob's account, whose head Carol cannot get from a chat of her own.
  await alice.page.getByRole("button", { name: /^People in Trio/ }).click();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Invite Carol" }).click();
  await expect(alice.page.getByRole("dialog").getByText("Invited")).toBeVisible();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Close" }).click();
  await joinGroup(carol, "Trio", "Alice");
  await say(carol, "carol here");
  await hears(alice, "carol here");
  await expect(roomLog(carol).getByText("hello bob", { exact: true })).toHaveCount(0); // a later joiner sees only later messages

  // Carol knows Alice and has no name for Bob; the Room gives nobody a name.
  await carol.page.getByRole("button", { name: /^People in Trio/ }).click();
  const people = carol.page.getByRole("dialog").getByRole("list", { name: "People in this group" });
  await expect(people.locator(".om-group-members__name")).toHaveCount(3);
  expect((await people.locator(".om-group-members__name").allTextContents()).map((t) => t.trim()).sort()).toEqual(["Alice", "Someone 1", "You"]); // the list order is Core's, not alphabetical
  await carol.page.getByRole("dialog").getByRole("button", { name: "Close" }).click();

  // Open question (see the notes of this change): Bob has no head for Carol's account, and a Commit carries no head for the devices it
  // adds, so Bob's Core keeps the Commit and says so instead of accepting it unverified. He is not told anything false.
  await openGroup(bob, "Trio");
  await expect(bob.page.getByRole("region", { name: "Group status" }).locator("visible=true").first().getByText("Waiting for a group update")).toBeVisible({ timeout: 60_000 });
  await expect(roomLog(bob).getByText("carol here", { exact: true })).toHaveCount(0);
  expectSealedOnly(alice, [/hello bob|carol here|Trio/]);
  expectSealedOnly(carol, [/hello bob|carol here|Trio/]);
});

test("three people who all know each other: join, names from each device's contacts, a removal and the route update after it", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  const carol = await joinByLink(browser, "Carol", await nextLink(alice), alice);
  everyone.push(carol);
  await meet(bob, carol, await nextLink(carol)); // now each of the three has the other two as contacts
  await readyGroups(bob);
  await readyGroups(carol);

  await createGroup(alice, "Trio", ["Bob"]);
  await joinGroup(bob, "Trio", "Alice");
  await openGroup(alice, "Trio");
  await say(alice, "hello bob");
  await hears(bob, "hello bob");
  await alice.page.getByRole("button", { name: /^People in Trio/ }).click();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Invite Carol" }).click();
  await expect(alice.page.getByRole("dialog").getByText("Invited")).toBeVisible();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Close" }).click();
  await joinGroup(carol, "Trio", "Alice");
  await say(carol, "carol here");
  await hears(alice, "carol here");
  await hears(bob, "carol here");
  await openGroup(carol, "Trio");
  await say(bob, "welcome carol");
  await hears(carol, "welcome carol");
  await hears(alice, "welcome carol");
  await say(alice, "hi both");
  await hears(carol, "hi both");
  await hears(bob, "hi both");
  await expect(roomLog(carol).getByText("hello bob", { exact: true })).toHaveCount(0);

  // Every device names the other two from its own contact list.
  const senders = async (p: Person): Promise<string[]> => (await roomLog(p).locator(".om-group-sender bdi").allTextContents()).map((t) => t.trim());
  expect(new Set(await senders(alice))).toEqual(new Set(["Bob", "Carol"]));
  expect(new Set(await senders(carol))).toEqual(new Set(["Alice", "Bob"]));
  expect(new Set(await senders(bob))).toEqual(new Set(["Alice", "Carol"]));

  // Alice removes Carol. Carol's Room goes away, and Alice and Bob go on, each minting a fresh route for the new roster (route update).
  const minted = (p: Person): number => p.wire.filter((w) => w.method === "POST" && w.url.endsWith("/v1/relay/queues")).length;
  const before = { alice: minted(alice), bob: minted(bob) };
  await alice.page.getByRole("button", { name: /^People in Trio/ }).click();
  const sheet = alice.page.getByRole("dialog");
  await sheet.getByRole("button", { name: "Remove Carol" }).click();
  await expect(sheet.getByRole("group", { name: "Remove Carol from Trio?" })).toBeVisible();
  await sheet.getByRole("group", { name: "Remove Carol from Trio?" }).getByRole("button", { name: "Remove", exact: true }).click();
  await expect(sheet.getByRole("list", { name: "People in this group" }).getByRole("listitem").filter({ hasText: "Carol" })).toHaveCount(0);
  await sheet.getByRole("button", { name: "Close" }).click();
  await expect(carol.page.getByRole("option", { name: /Trio/ })).toHaveCount(0, { timeout: 60_000 });
  await expect.poll(() => minted(alice), { timeout: 60_000 }).toBeGreaterThan(before.alice);
  await expect.poll(() => minted(bob), { timeout: 60_000 }).toBeGreaterThan(before.bob);
  await say(alice, "after the removal");
  await hears(bob, "after the removal");
  await openGroup(bob, "Trio");
  await say(bob, "still here");
  await hears(alice, "still here");
  await carol.page.waitForTimeout(3_000);
  await expect(carol.page.getByText("after the removal")).toHaveCount(0);
  expectSealedOnly(alice, [/hello bob|carol here|welcome carol|after the removal|still here|Trio/]);
  expectSealedOnly(carol, [/hello bob|carol here|welcome carol|after the removal|still here|Trio/]);
});

test("a held invitation: the device has no groups module yet, the reload keeps the entry, tapping it gets groups ready and then Join is asked", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  expect(bob.wasm.filter((u) => /om_wasm_groups/.test(u))).toEqual([]); // Bob's device has only the base module

  await createGroup(alice, "Chess club", ["Bob"]);

  // Bob's base Core verifies the invitation and holds it: a calm entry, nothing accepted, nobody told.
  const held = bob.page.getByRole("region", { name: "Group invitations waiting" });
  await expect(held.getByText("You have a group invitation")).toBeVisible({ timeout: 60_000 });
  await expect(held.getByText("Alice invited you to Chess club")).toBeVisible();
  await expect(bob.page.getByRole("button", { name: "Join" })).toHaveCount(0);

  // Bob closes the page and opens it again while the invitation is held and the module is still not loaded: the entry is still there.
  await bob.page.reload();
  await reopen(bob.page);
  await expect(held.getByText("Alice invited you to Chess club")).toBeVisible({ timeout: 60_000 });
  expect(bob.wasm.filter((u) => /om_wasm_groups/.test(u))).toEqual([]);

  // One tap gets the groups ready (fetched, checked, kept, restart); the ordinary card then asks Join or No thanks.
  await held.getByRole("button", { name: "Open invitation" }).click();
  const card = bob.page.getByRole("region", { name: "Group invitations", exact: true }).locator(".om-group-invite").filter({ hasText: "Chess club" });
  await expect(card.getByText("Alice invited you to Chess club")).toBeVisible({ timeout: 90_000 });
  expect(bob.wasm.filter((u) => /om_wasm_groups/.test(u)).length).toBeGreaterThan(0);
  await expect(held).toHaveCount(0);
  await expect(bob.page.getByRole("option", { name: /Chess club/ })).toHaveCount(0); // loading joined nothing
  await card.getByRole("button", { name: "Join" }).click();
  await openGroup(bob, "Chess club");
  await expect(bob.page.getByRole("textbox", { name: "Message the group" })).toBeEnabled({ timeout: 60_000 });
  await say(bob, "joined after a reload");
  await openGroup(alice, "Chess club");
  await hears(alice, "joined after a reload");
  await say(alice, "welcome");
  await hears(bob, "welcome");
  expectSealedOnly(bob, [/joined after a reload|welcome|Chess club/]);
});
