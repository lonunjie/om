// Everyday chat gestures in a Room of three people on the REAL Core with the OpenMLS engine, over the real carriage on the local reference
// Home, each person in a browser context of their own: reply (quote and chip), reactions counted per member, edit and delete for everyone
// of one's own messages only, and a removed member who can no longer take part (D143). Only what a person can do on screen is driven.
import { expect, test } from "@playwright/test";
import {
  closeAll, createGroup, expectSealedOnly, firstLink, hears, joinByLink, joinGroup, meet, nextLink, openGroup, readyGroups, roomLog, say, startPerson, type Person,
} from "./groups-live-people.ts";

test.use({ baseURL: "http://localhost:4180" });
test.setTimeout(300_000);

const everyone: Person[] = [];
test.afterEach(async () => { await closeAll(everyone.splice(0)); });

const thumbs = "\u{1F44D}";

test("in a Room of three: reply, reactions, edit and delete work for everyone, only the author can change a message, and a removed member is out", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  const carol = await joinByLink(browser, "Carol", await nextLink(alice), alice);
  everyone.push(carol);
  await meet(bob, carol, await nextLink(carol));
  await readyGroups(bob);
  await readyGroups(carol);

  await createGroup(alice, "Trio", ["Bob"]);
  await joinGroup(bob, "Trio", "Alice");
  await openGroup(alice, "Trio");
  await say(alice, "dinner at eight?");
  await hears(bob, "dinner at eight?");
  await alice.page.getByRole("button", { name: /^People in Trio/ }).click();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Invite Carol" }).click();
  await expect(alice.page.getByRole("dialog").getByText("Invited")).toBeVisible();
  await alice.page.getByRole("dialog").getByRole("button", { name: "Close" }).click();
  await joinGroup(carol, "Trio", "Alice");
  await openGroup(bob, "Trio");
  await say(alice, "carol, are you in?");
  await hears(carol, "carol, are you in?");
  await hears(bob, "carol, are you in?");

  // Reply: Carol answers Alice's question; the chip names who she answers, and everyone sees the quote in their own names.
  await carol.page.getByRole("button", { name: "Reply: carol, are you in?" }).click();
  await expect(carol.page.getByRole("group", { name: "Replying to Alice" })).toBeVisible();
  await carol.page.getByRole("textbox", { name: "Message the group" }).fill("yes, I am");
  await carol.page.getByRole("button", { name: "Send" }).click();
  await expect(carol.page.getByRole("group", { name: "Replying to Alice" })).toHaveCount(0);
  await expect(roomLog(carol).getByRole("button", { name: /^In reply to Alice: carol, are you in\?/ })).toBeVisible({ timeout: 60_000 });
  await expect(roomLog(alice).getByRole("button", { name: /^In reply to you: carol, are you in\?/ })).toBeVisible({ timeout: 60_000 });
  await expect(roomLog(bob).getByRole("button", { name: /^In reply to Alice: carol, are you in\?/ })).toBeVisible({ timeout: 60_000 });
  await roomLog(alice).getByRole("button", { name: /^In reply to you/ }).click();
  await expect(alice.page.locator("[data-found='true']")).toContainText("carol, are you in?");

  // Reactions are per member: Bob and Carol both react to Alice's question; each sees which one is theirs.
  const q = "carol, are you in?";
  await bob.page.getByRole("button", { name: `React: ${q}` }).click();
  await bob.page.getByRole("button", { name: `React with ${thumbs}` }).click();
  await expect(bob.page.getByRole("button", { name: `${thumbs} 1, including you. Remove your reaction` })).toBeVisible({ timeout: 30_000 });
  await expect(alice.page.getByRole("button", { name: `${thumbs} 1. Add yours` })).toBeVisible({ timeout: 60_000 });
  await expect(carol.page.getByRole("button", { name: `${thumbs} 1. Add yours` })).toBeVisible({ timeout: 60_000 });
  await carol.page.getByRole("button", { name: `${thumbs} 1. Add yours` }).click();
  await expect(carol.page.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` })).toBeVisible({ timeout: 30_000 });
  await expect(alice.page.getByRole("button", { name: `${thumbs} 2. Add yours` })).toBeVisible({ timeout: 60_000 });
  await expect(bob.page.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` })).toBeVisible({ timeout: 60_000 });
  await bob.page.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` }).click();
  await expect(alice.page.getByRole("button", { name: `${thumbs} 1. Add yours` })).toBeVisible({ timeout: 60_000 });

  // Edit: only Alice's own message offers it; everyone sees the new text marked Edited, and the quote in Carol's reply follows it.
  await expect(bob.page.getByRole("button", { name: /^(Edit|Delete): carol, are you in\?/ })).toHaveCount(0);
  await expect(carol.page.getByRole("button", { name: /^(Edit|Delete): carol, are you in\?/ })).toHaveCount(0);
  await alice.page.getByRole("button", { name: `Edit: ${q}` }).click();
  await alice.page.getByRole("textbox", { name: "Edit message" }).fill("carol, are you in tonight?");
  await alice.page.getByRole("button", { name: "Save" }).click();
  for (const p of [alice, bob, carol]) await expect(roomLog(p).locator(".om-group-text").filter({ hasText: /^carol, are you in tonight\?$/ })).toBeVisible({ timeout: 60_000 });
  await expect(roomLog(bob).getByText("Edited")).toBeVisible();
  await expect(roomLog(carol).getByText("Edited")).toBeVisible();
  await expect(roomLog(bob).getByRole("button", { name: /^In reply to Alice: carol, are you in tonight\?/ })).toBeVisible();

  // Delete for everyone, after being told what it means; the reply to it then says its original is gone.
  await alice.page.getByRole("button", { name: "Delete: carol, are you in tonight?" }).click();
  await expect(alice.page.getByRole("group", { name: "Delete for everyone?" })).toContainText("may still have a copy");
  await alice.page.getByRole("button", { name: "Delete for everyone" }).click();
  for (const p of [alice, bob, carol]) await expect(roomLog(p).locator(".om-group-text").filter({ hasText: /^carol, are you in tonight\?$/ })).toHaveCount(0, { timeout: 60_000 });
  await expect(roomLog(bob).getByText("Original message unavailable")).toBeVisible({ timeout: 30_000 });
  await expect(roomLog(carol).getByText("yes, I am", { exact: true })).toBeVisible();

  // A removed member can no longer take part: Carol's Room goes away, Alice and Bob go on, and nothing she does reaches them.
  await alice.page.getByRole("button", { name: /^People in Trio/ }).click();
  const sheet = alice.page.getByRole("dialog");
  await sheet.getByRole("button", { name: "Remove Carol" }).click();
  await sheet.getByRole("group", { name: "Remove Carol from Trio?" }).getByRole("button", { name: "Remove", exact: true }).click();
  await expect(sheet.getByRole("list", { name: "People in this group" }).getByRole("listitem").filter({ hasText: "Carol" })).toHaveCount(0);
  await sheet.getByRole("button", { name: "Close" }).click();
  await expect(carol.page.getByRole("option", { name: /Trio/ })).toHaveCount(0, { timeout: 60_000 });
  await bob.page.getByRole("button", { name: "React: dinner at eight?" }).click();
  await bob.page.getByRole("button", { name: `React with ${thumbs}` }).click();
  await expect(alice.page.getByRole("button", { name: `${thumbs} 1. Add yours` })).toBeVisible({ timeout: 60_000 });
  await carol.page.waitForTimeout(3_000);
  await expect(carol.page.getByText("dinner at eight?")).toHaveCount(0);
  const seen = [/dinner at eight|are you in|yes, I am|Edited|Trio/];
  expectSealedOnly(alice, seen);
  expectSealedOnly(bob, seen);
  expectSealedOnly(carol, seen);
});

test("in a Room: a mention picks a member from the list, reaches every member, and marks the one who was named", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  await createGroup(alice, "Duo", ["Bob"]);
  await joinGroup(bob, "Duo", "Alice");
  await openGroup(alice, "Duo");
  await openGroup(bob, "Duo");

  const box = alice.page.getByRole("textbox", { name: "Message the group" });
  await box.fill("thanks @Bo");
  await expect(alice.page.getByRole("listbox", { name: "People you can mention" })).toBeVisible();
  await alice.page.getByRole("option", { name: "Bob" }).click();
  await box.pressSequentially("for the lift");
  await alice.page.getByRole("button", { name: "Send" }).click();

  await hears(bob, "thanks @Bob for the lift");
  await expect(roomLog(bob).locator(".om-richtext__mention[data-me='true']")).toHaveText(/@Bob/);
  await expect(roomLog(alice).locator(".om-richtext__mention")).toHaveText(/@Bob/);
  await expect(roomLog(alice).locator(".om-richtext__mention[data-me]")).toHaveCount(0);
});

test("in a Room: a poll is created, voted in, changed and closed, and every member sees the same tally", async ({ browser }) => {
  const alice = await startPerson(browser, "Alice");
  everyone.push(alice);
  const bob = await joinByLink(browser, "Bob", await firstLink(alice), alice);
  everyone.push(bob);
  await createGroup(alice, "Poll room", ["Bob"]);
  await joinGroup(bob, "Poll room", "Alice");
  await openGroup(alice, "Poll room");
  await openGroup(bob, "Poll room");

  await alice.page.getByRole("button", { name: "Create a poll" }).click();
  const sheet = alice.page.getByRole("dialog", { name: "New poll" });
  await sheet.getByRole("textbox", { name: "Question" }).fill("Where do we eat?");
  await sheet.getByRole("textbox", { name: "Option 1" }).fill("Pizza");
  await sheet.getByRole("textbox", { name: "Option 2" }).fill("Noodles");
  await expect(sheet.getByText(/Votes are not secret/)).toBeVisible();
  await sheet.getByRole("button", { name: "Create poll" }).click();

  const bobPoll = roomLog(bob).getByRole("group", { name: "Where do we eat?" });
  await expect(bobPoll).toBeVisible({ timeout: 90_000 });
  await bobPoll.getByRole("radio", { name: "Noodles" }).check();
  await bobPoll.getByRole("button", { name: "Vote" }).click();
  const alicePoll = roomLog(alice).getByRole("group", { name: "Where do we eat?" });
  await expect(alicePoll.getByText("1 person has voted")).toBeVisible({ timeout: 90_000 });
  await expect(alicePoll.locator(".om-poll__option", { hasText: "Noodles" })).toContainText("1 vote");

  // Bob changes his mind, then the author closes: nobody can vote after that and both see the closed poll.
  await bobPoll.getByRole("radio", { name: /Pizza/ }).check();
  await bobPoll.getByRole("button", { name: "Change vote" }).click();
  await expect(alicePoll.locator(".om-poll__option", { hasText: "Pizza" })).toContainText("1 vote", { timeout: 90_000 });
  await alicePoll.getByRole("button", { name: "Close poll" }).click();
  await alicePoll.getByRole("button", { name: "Close for everyone" }).click();
  await expect(bobPoll.getByText("Poll closed")).toBeVisible({ timeout: 90_000 });
  await expect(bobPoll.getByRole("radio", { name: /Pizza/ })).toBeDisabled();
});
