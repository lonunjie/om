// Everyday chat gestures between two people on the real Core, through the local reference Home: reply (quote, chip, jump).
// The same two people and the same session carry the later gestures in this file.
import { expect, test } from "@playwright/test";
import { inviteAndChat } from "./invite-flow.ts";

const BASE = "http://localhost:4177";
test.use({ baseURL: BASE });

test("a reply quotes the original for both people, and a tap on the quote moves to it", async ({ browser }) => {
  const { alice, bob } = await inviteAndChat(browser, BASE);

  // Bob answers Alice's "hello bob": Reply puts a chip above the box, Send clears it.
  await bob.getByRole("button", { name: "Reply: hello bob" }).click();
  await expect(bob.getByText("Replying to Alice")).toBeVisible();
  await bob.getByRole("textbox", { name: /message/i }).fill("hello back, with a quote");
  await bob.getByRole("button", { name: /send/i }).click();
  await expect(bob.getByText("Replying to Alice")).toHaveCount(0);

  // Bob's own timeline shows the quote of Alice's message.
  await expect(bob.getByRole("log").getByRole("button", { name: /^In reply to Alice: hello bob/ })).toBeVisible({ timeout: 60_000 });
  // Alice receives it: the quote resolves to her own message.
  const quote = alice.getByRole("log").getByRole("button", { name: /^In reply to you: hello bob/ });
  await expect(quote).toBeVisible({ timeout: 60_000 });
  await expect(alice.getByRole("log").getByText("hello back, with a quote")).toBeVisible();

  // Tapping the quote moves to the original and marks it for a moment.
  await quote.click();
  await expect(alice.locator("[data-found='true']")).toContainText("hello bob");

  // Escape drops a reply that was started and not sent.
  await alice.getByRole("button", { name: "Reply: hi alice" }).click();
  await expect(alice.getByText("Replying to Bob")).toBeVisible();
  await alice.getByRole("textbox", { name: /message/i }).press("Escape");
  await expect(alice.getByText("Replying to Bob")).toHaveCount(0);
});

test("a reaction reaches the other person, counts both people, and either can remove their own", async ({ browser }) => {
  const { alice, bob } = await inviteAndChat(browser, BASE);
  const thumbs = "\u{1F44D}";

  // Bob reacts to Alice's "hello bob" from the picker.
  await bob.getByRole("button", { name: "React: hello bob" }).click();
  await expect(bob.getByRole("group", { name: "Choose a reaction" }).getByRole("button")).toHaveCount(6);
  await bob.getByRole("button", { name: `React with ${thumbs}` }).click();
  await expect(bob.getByRole("button", { name: `${thumbs} 1, including you. Remove your reaction` })).toBeVisible({ timeout: 30_000 });
  // Alice sees it on her own message, as someone else's.
  const theirs = alice.getByRole("button", { name: `${thumbs} 1. Add yours` });
  await expect(theirs).toBeVisible({ timeout: 60_000 });

  // Alice adds hers by tapping the chip: both sides count two.
  await theirs.click();
  await expect(alice.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` })).toBeVisible({ timeout: 30_000 });
  await expect(bob.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` })).toBeVisible({ timeout: 60_000 });

  // Bob removes his: Alice's chip is hers alone again.
  await bob.getByRole("button", { name: `${thumbs} 2, including you. Remove your reaction` }).click();
  await expect(alice.getByRole("button", { name: `${thumbs} 1, including you. Remove your reaction` })).toBeVisible({ timeout: 60_000 });
  await expect(bob.getByRole("button", { name: `${thumbs} 1. Add yours` })).toBeVisible({ timeout: 30_000 });
});

test("an edit shows on both sides marked Edited, and delete for everyone removes the message for both", async ({ browser }) => {
  const { alice, bob } = await inviteAndChat(browser, BASE);

  // Alice fixes the text of her first message.
  await alice.getByRole("button", { name: "Edit: hello bob" }).click();
  const box = alice.getByRole("textbox", { name: "Edit message" });
  await box.fill("hello bob, fixed");
  await alice.getByRole("button", { name: "Save" }).click();
  await expect(alice.getByRole("log").getByText("hello bob, fixed")).toBeVisible({ timeout: 30_000 });
  await expect(bob.getByRole("log").getByText("hello bob, fixed")).toBeVisible({ timeout: 60_000 });
  await expect(bob.getByRole("log").getByText("Edited")).toBeVisible();
  await expect(alice.getByRole("log").getByText("Edited")).toBeVisible();
  // Bob has no Edit or Delete on Alice's message.
  await expect(bob.getByRole("button", { name: /^(Edit|Delete): hello bob/ })).toHaveCount(0);

  // Alice deletes it for everyone, after being told what that means.
  await alice.getByRole("button", { name: "Delete: hello bob, fixed" }).click();
  await expect(alice.getByRole("group", { name: "Delete for everyone?" })).toContainText("may still have a copy");
  await alice.getByRole("button", { name: "Delete for everyone" }).click();
  await expect(alice.getByRole("log").getByText("hello bob, fixed")).toHaveCount(0, { timeout: 30_000 });
  await expect(bob.getByRole("log").getByText("hello bob, fixed")).toHaveCount(0, { timeout: 60_000 });
  await expect(bob.getByRole("log").getByText("hi alice")).toBeVisible();
});

test("a mention names the other person, reaches them marked as for them, and stays readable text", async ({ browser }) => {
  const { alice, bob } = await inviteAndChat(browser, BASE);

  // Alice writes "@", picks Bob from the list with the keyboard, and sends.
  const box = alice.getByRole("textbox", { name: /message/i });
  await box.fill("hi @Bo");
  await expect(alice.getByRole("listbox", { name: "People you can mention" })).toBeVisible();
  await box.press("Enter");
  await expect(box).toHaveValue("hi @Bob ");
  await box.pressSequentially("lunch?");
  await alice.getByRole("button", { name: /send/i }).click();

  // Bob sees the words, a mention span, and a spoken cue that it names him; Alice sees the span without the cue.
  const mention = bob.getByRole("log").locator(".om-richtext__mention");
  await expect(mention).toHaveText(/@Bob/, { timeout: 60_000 });
  await expect(mention).toHaveAttribute("data-me", "true");
  await expect(bob.getByRole("log")).toContainText("hi @Bob lunch?");
  await expect(alice.getByRole("log").locator(".om-richtext__mention")).toHaveText(/@Bob/);
  await expect(alice.getByRole("log").locator(".om-richtext__mention[data-me]")).toHaveCount(0);
});

test("a scheduled message is held on this device and goes out at the chosen time as an ordinary message", async ({ browser }) => {
  const { alice, bob } = await inviteAndChat(browser, BASE);
  const box = alice.getByRole("textbox", { name: /message/i });
  await box.fill("scheduled hello");
  await alice.getByRole("button", { name: "Schedule send" }).click();
  const form = alice.getByRole("form", { name: "Schedule this message" });
  await expect(form).toContainText("only while OM is open on this device");
  const at = new Date(Date.now() + 50_000);
  const pad = (n: number): string => String(n).padStart(2, "0");
  await form.getByLabel("Send at").fill(`${String(at.getFullYear())}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}T${pad(at.getHours())}:${pad(at.getMinutes())}`);
  await form.getByRole("button", { name: "Schedule" }).click();
  await expect(alice.getByRole("region", { name: "Scheduled messages" })).toContainText("scheduled hello");
  await expect(bob.getByRole("log").getByText("scheduled hello")).toHaveCount(0);
  // It leaves at the chosen minute (the form is minute-granular) and arrives as a normal message; the queue empties.
  await expect(bob.getByRole("log").getByText("scheduled hello")).toBeVisible({ timeout: 150_000 });
  await expect(alice.getByRole("region", { name: "Scheduled messages" })).toHaveCount(0, { timeout: 30_000 });
});
