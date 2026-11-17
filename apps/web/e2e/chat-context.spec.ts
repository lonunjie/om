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
