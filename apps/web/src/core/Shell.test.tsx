// @vitest-environment jsdom
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { createHarness } from "@om/core-harness";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createActionRegistry, createMemoryKeyValue, createProfileScopedState, ProfileProvider } from "@om/ui";
import { Shell } from "./Shell";

afterEach(cleanup);

const lookalike = "Mariа"; // The last letter is Cyrillic.
const contact = (id: string, displayName: string, message?: string, nickname?: string) => ({
  id: asConversationId(id), personId: asPersonId(id), displayName, ...(nickname === undefined ? {} : { nickname }),
  messages: message === undefined ? [] : [{ direction: "incoming" as const, text: message }],
});
const introduction = (id: string, displayName: string) => ({ id, from: asPersonId(id), personId: asPersonId(id), displayName });

async function mount(conversations: ReturnType<typeof contact>[], introductions: ReturnType<typeof introduction>[] = []) {
  window.history.replaceState(null, "", "/");
  const profileId = asProfileId("me");
  const client = createHarness({ env: {}, clock: () => 1000 });
  client.seedProfile({ profileId, label: "Me", conversations, introductions });
  const services = { client, registry: createActionRegistry(), ctx: client.contextFor(profileId) };
  for (const c of conversations) {
    if (c.nickname !== undefined) await client.dispatch({ kind: "SetContactNickname", ctx: services.ctx, operationId: asOperationId(`nick-${c.id}`), conversationId: c.id, nickname: c.nickname });
  }
  const store = createProfileScopedState(createMemoryKeyValue());
  return render(<ProfileProvider client={client} contexts={client} store={store} initialProfileId={profileId}><Shell services={services} /></ProfileProvider>);
}

describe("name cues", () => {
  it("warns about exact and similar names among pending introductions before acceptance", async () => {
    await mount([], [introduction("one", "Sam"), introduction("two", "SAM"), introduction("three", "Maria"), introduction("four", lookalike), introduction("five", "Ana")]);
    const requests = await screen.findAllByRole("region", { name: "Message request" });
    expect(requests).toHaveLength(5);
    expect(screen.getAllByText(/people here are named Sam/i)).toHaveLength(2);
    expect(screen.getAllByText(/names that look similar to/i)).toHaveLength(2);
    expect(requests.every((r) => !within(r).getByRole<HTMLButtonElement>("button", { name: "Accept" }).disabled)).toBe(true);
    expect(requests[4]?.querySelector("[data-samename]")).toBeNull();
  });

  it("announces exact and similar names on local chat and message search results", async () => {
    await mount([contact("1", "Sam", "first message"), contact("2", "SAM"), contact("3", "Maria", "second message"), contact("4", lookalike), contact("5", "Sam", "third message", "Annie")]);
    await screen.findByRole("option", { name: /Annie/ });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Omnibar/ }));
    const box = screen.getByRole("combobox", { name: "Omnibar" });
    await user.type(box, "sam");
    const results = within(screen.getByRole("listbox", { name: "Results" }));
    expect(results.getAllByRole("option", { name: /Same name as another contact/i })).toHaveLength(2);
    await user.clear(box);
    await user.type(box, "message");
    expect(results.getByRole("option", { name: /first message.*same name as another contact/i })).toBeTruthy();
    expect(results.getByRole("option", { name: /second message.*similar to another contact/i })).toBeTruthy();
    expect(results.getByRole("option", { name: /third message/ }).textContent).not.toMatch(/another contact/i);
    const a11y = await axe.run(screen.getByRole("dialog", { name: "Omnibar" }), { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
    expect(a11y.violations).toEqual([]);
  });
});
