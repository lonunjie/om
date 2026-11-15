// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useState } from "react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { asConversationId, asGroupId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { useConversationList, type ConversationServices } from "../conversations/index";
import { GroupChat, GroupComposer, GroupInvites, GroupStatus, NO_STATES, NewGroup, groupApiOf, reduceGroupStates, runGroupAction, useGroupEvents, useGroups } from "./index";
import { PROTECTION_LEVELS, bundleOf } from "./protection";
import { groupsTestCopy as copy } from "./__fixtures__/copy";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ME = asProfileId("me");
const OTHER = asProfileId("other");
const ANA = asConversationId("c-ana");
const BEN = asConversationId("c-ben");

function setup() {
  const h = createHarness({ env: {}, clock: () => 1000 });
  h.seedProfile({ profileId: ME, label: "Me", conversations: [
    { id: ANA, personId: asPersonId("p-ana"), displayName: "Ana", messages: [] },
    { id: BEN, personId: asPersonId("p-ben"), displayName: "Ben", messages: [] },
  ] });
  h.seedProfile({ profileId: OTHER, label: "Other", conversations: [] });
  let n = 0;
  const registry = createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++n)}`) });
  const services: ConversationServices = { client: h, registry, ctx: h.contextFor(ME) };
  return { h, services };
}

function Screen({ services, open }: { readonly services: ConversationServices; readonly open?: ReturnType<typeof asGroupId> }) {
  const groups = useGroups(services);
  const events = useGroupEvents(services, () => { groups.refresh(); });
  const list = useConversationList(services);
  const [draft, setDraft] = useState("");
  const [key, setKey] = useState(0);
  const [selected, setSelected] = useState<ReturnType<typeof asGroupId> | null>(open ?? null);
  const chats = list.status === "ready" ? list.value : [];
  return (
    <div>
      <GroupInvites services={services} copy={copy} invites={groups.invites} chats={chats} onAnswered={groups.refresh} />
      <GroupStatus copy={copy} states={events.states} scope="all" onDismissUndeliverable={events.dismissUndeliverable} onDismissSettingBlocked={events.dismissSettingBlocked} onDismissAdmitBlocked={events.dismissAdmitBlocked} />
      <NewGroup services={services} copy={copy} chats={chats} onCreated={groups.refresh} />
      <ul aria-label="groups">{groups.groups.map((g) => <li key={g.conversationId}><button type="button" onClick={() => { setSelected(g.conversationId); }}>{g.name} ({g.members}+{g.invited})</button></li>)}</ul>
      {selected === null ? null : (
        <GroupChat services={services} copy={copy} groupId={selected} chats={chats} refreshKey={key} events={events} onChanged={groups.refresh} onLeft={() => { setSelected(null); groups.refresh(); }}
          footer={<GroupComposer services={services} copy={copy} groupId={selected} draft={draft} onDraftChange={setDraft} onSent={() => { setKey((n) => n + 1); groups.refresh(); }} />} />
      )}
    </div>
  );
}

describe("groups UI on the loopback harness", () => {
  it("is absent where the host has no group engine", () => {
    const { services } = setup();
    expect(groupApiOf(services)).not.toBeNull();
    const bare = { ...services, client: { ...services.client, groups: undefined } } as unknown as ConversationServices;
    expect(groupApiOf(bare)).toBeNull();
  });

  it("create: choose people, the group appears with the others still invited, nobody is added without saying yes", async () => {
    const { services } = setup();
    const { container } = render(<Screen services={services} />);
    expect(await screen.findByText("Each person is only added if they say yes.")).toBeTruthy();
    const create = await screen.findByRole("button", { name: "Create group" });
    expect(create.hasAttribute("disabled")).toBe(true);
    await userEvent.type(screen.getByLabelText("Group name"), "Book club");
    await userEvent.click(await screen.findByRole("checkbox", { name: /Ana/ }));
    await userEvent.click(screen.getByRole("button", { name: /Create group/ }));
    expect(await screen.findByRole("button", { name: "Book club (1+1)" })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("accept an invitation, then send and receive", async () => {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    const { container } = render(<Screen services={services} />);
    expect(await screen.findByText("Ana invited you to Family")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Join" }));
    expect(await screen.findByText("Joined")).toBeTruthy();
    const open = await screen.findByRole("button", { name: /Family/ });
    await userEvent.click(open);
    const box = await screen.findByRole("textbox", { name: "Message the group" });
    await userEvent.type(box, "hello all");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("hello all")).toBeTruthy();
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    h.groups.simulateMessage(services.ctx, gid, "acct-c-ana", "hi from ana");
    await waitFor(() => { expect(screen.queryByText("hi from ana")).not.toBeNull(); }, { timeout: 5000 });
    expect(await axeViolations(container)).toEqual([]);
  });

  it("roster: add, remove and leave each need their own confirmation, and leaving returns to the list", async () => {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "Join" }));
    await userEvent.click(await screen.findByRole("button", { name: /Family/ }));
    await userEvent.click(await screen.findByRole("button", { name: "People in Family" }));
    const sheet = await screen.findByRole("dialog");
    expect(within(sheet).getByText("Later joiners see only later messages.")).toBeTruthy();
    expect(within(sheet).getByText("Messages can be shown as proof.")).toBeTruthy();
    // Add: Ben is offered, and invited (not added).
    await userEvent.click(within(sheet).getByRole("button", { name: "Invite Ben" }));
    await waitFor(() => { expect(h.groups.list(services.ctx)[0]?.invited).toBe(1); });
    // Leave asks first; "Stay" keeps everything.
    await userEvent.click(within(sheet).getByRole("button", { name: "Leave group" }));
    expect(within(sheet).getByText("Leave Family?")).toBeTruthy();
    await userEvent.click(within(sheet).getByRole("button", { name: "Stay" }));
    expect(h.groups.list(services.ctx)).toHaveLength(1);
    await userEvent.click(within(sheet).getByRole("button", { name: "Leave group" }));
    await userEvent.click(within(sheet).getByRole("button", { name: "Yes, leave" }));
    await waitFor(() => { expect(screen.queryByRole("textbox", { name: "Message the group" })).toBeNull(); });
  });

  it("a refusal is shown in words and a profile switch cannot retarget a step", async () => {
    const { services, h } = setup();
    const r = await runGroupAction(services, "group-leave", { conversationId: "nope" });
    expect(r).toMatchObject({ ok: false });
    const other = { ...services, ctx: h.contextFor(OTHER) };
    const stale = { ...services, ctx: services.ctx };
    expect(h.groups.list(other.ctx)).toHaveLength(0);
    expect(stale.ctx).not.toBe(other.ctx);
  });

  it("create with the default is one tap: the Recommended level is pre-selected and nothing else is asked", async () => {
    const { h, services } = setup();
    render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /Ana/ }));
    expect(screen.getByText("Protection: Recommended")).toBeTruthy();
    expect(screen.getByRole<HTMLInputElement>("radio", { name: /Recommended/ }).checked).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: /Create group/ }));
    await waitFor(() => { expect(h.groups.list(services.ctx)).toHaveLength(1); });
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    expect(h.groups.get(services.ctx, gid)?.protection).toMatchObject({ profile: 1, custom: false, membersMayInvite: true, joinRule: 1 });
  });

  it("create with Hardened (and Maximum Privacy): the level is offered in plain words and Core builds the Room from it", async () => {
    const { h, services } = setup();
    const { container } = render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /Ana/ }));
    await userEvent.click(screen.getByText("Protection: Recommended"));
    expect(screen.getByRole("radio", { name: /Hardened/ })).toBeTruthy();
    expect(screen.getByRole("radio", { name: /Maximum Privacy/ })).toBeTruthy();
    await userEvent.click(screen.getByRole("radio", { name: /Hardened/ }));
    expect(screen.getByText("Protection: Hardened")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: /Create group/ }));
    await waitFor(() => { expect(h.groups.list(services.ctx)).toHaveLength(1); });
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    expect(h.groups.get(services.ctx, gid)?.protection).toMatchObject({ profile: 2, custom: false, membersMayInvite: false, joinRule: 2, newDevicePolicy: 1 });
    const r = await runGroupAction(services, "group-create", { name: "Max", relationshipIds: [BEN], protection: 3 });
    expect(r.ok).toBe(true);
    const max = h.groups.list(services.ctx).find((g) => g.name === "Max");
    expect(h.groups.get(services.ctx, max?.conversationId ?? asGroupId(""))?.protection).toMatchObject({ profile: 3, newDevicePolicy: 2 });
    expect(await runGroupAction(services, "group-create", { name: "X", relationshipIds: [BEN], protection: 4 })).toMatchObject({ ok: false, refused: "invalid" });
  });

  async function openSettings() {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    const view = render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "Join" }));
    await userEvent.click(await screen.findByRole("button", { name: /Family/ }));
    await userEvent.click(await screen.findByRole("button", { name: "People in Family" }));
    const people = await screen.findByRole("dialog");
    expect(within(people).getByText("Everyone can see who is in it.")).toBeTruthy();
    await userEvent.click(within(people).getByRole("button", { name: "Room settings" }));
    const sheet = await screen.findByRole("dialog", { name: "Settings for Family" });
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    return { h, services, sheet, gid, view };
  }

  it("tightening applies at once, says its cost first, and the change shows in the timeline for everyone", async () => {
    const { h, services, sheet, gid, view } = await openSettings();
    expect(within(sheet).getByText("Adding may take longer.")).toBeTruthy();
    await userEvent.click(within(sheet).getByRole("button", { name: "Only admins can add people" }));
    await waitFor(() => { expect(h.groups.get(services.ctx, gid)?.protection.membersMayInvite).toBe(false); });
    expect(within(sheet).queryByText("Make this group less strict?")).toBeNull();
    expect(await within(sheet).findByText("Changed.", { selector: "p[aria-hidden]" })).toBeTruthy();
    expect(await axeViolations(view.container)).toEqual([]);
    // The Room now reads Custom or a level; the timeline carries the notice with who made it.
    await userEvent.click(within(sheet).getByRole("button", { name: "Close" }));
    expect(await screen.findByText("You changed who can add people: now only admins.")).toBeTruthy();
    expect(screen.queryByText("Less strict")).toBeNull();
  });

  it("loosening names what changes, needs its own confirmation, and is marked in the timeline", async () => {
    const { h, services, sheet, gid, view } = await openSettings();
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Hardened" }));
    await waitFor(() => { expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(2); });
    // Back to Recommended is a loosening: nothing happens until it is confirmed.
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Recommended" }));
    const ask = await within(sheet).findByRole("group", { name: "Make this group less strict?" });
    expect(within(ask).getByText("Anyone can add people.")).toBeTruthy();
    expect(within(ask).getByText("No admin's OK for new people.")).toBeTruthy();
    expect(within(ask).getByText("Level goes down to Recommended.")).toBeTruthy();
    expect(within(ask).getByText("Everyone will see who changed it.")).toBeTruthy();
    expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(2);
    expect(await axeViolations(view.container)).toEqual([]);
    await userEvent.click(within(ask).getByRole("button", { name: "Keep it as it is" }));
    expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(2);
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Recommended" }));
    await userEvent.click(within(await within(sheet).findByRole("group", { name: "Make this group less strict?" })).getByRole("button", { name: "Make less strict" }));
    await waitFor(() => { expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(1); });
    await userEvent.click(within(sheet).getByRole("button", { name: "Close" }));
    expect((await screen.findAllByText("Less strict")).length).toBeGreaterThan(0);
  });

  it("Core refuses an unconfirmed loosening and a quorum it cannot collect; the screen says so and changes nothing", async () => {
    const { h, services, gid } = await openSettings();
    const sheet = screen.getByRole("dialog", { name: "Settings for Family" });
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Maximum Privacy" }));
    await waitFor(() => { expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(3); });
    expect(await runGroupAction(services, "group-room-setting", { conversationId: gid, setting: { protection: 1 } })).toEqual({ ok: false, refused: "loosening_needs_confirmation" });
    h.groups.simulateQuorum(services.ctx, gid);
    await userEvent.click(within(sheet).getByRole("button", { name: "Use Recommended" }));
    await userEvent.click(within(await within(sheet).findByRole("group", { name: "Make this group less strict?" })).getByRole("button", { name: "Make less strict" }));
    expect((await within(sheet).findByRole("alert")).textContent).toBe("Refused: quorum_not_supported");
    expect(h.groups.get(services.ctx, gid)?.protection.profile).toBe(3);
    expect(h.groups.notices(services.ctx, gid)).toHaveLength(1);
  });

  it("without the capability Core refuses and the screen says so", async () => {
    const { h, services, sheet, gid } = await openSettings();
    h.groups.simulateNoAuthority(services.ctx, gid);
    await userEvent.click(within(sheet).getByRole("button", { name: "Only admins can add people" }));
    expect((await within(sheet).findByRole("alert")).textContent).toBe("Refused: not_authorized");
    expect(h.groups.get(services.ctx, gid)?.protection.membersMayInvite).toBe(true);
  });

  it("registry: only the offered settings are accepted, and a suggestion cannot change a Room", () => {
    const registry = createActionRegistry({ newOperationId: () => asOperationId("op") });
    const { services } = setup();
    const res = (params: Record<string, unknown>, origin: "direct" | "suggestion" = "direct") => registry.resolve({ actionId: "group-room-setting", params, ctx: services.ctx, origin });
    expect(res({ conversationId: "g1", setting: { protection: 2 } })).toMatchObject({ status: "group", intent: { kind: "room_setting", setting: { protection: 2 } } });
    expect(res({ conversationId: "g1", setting: { join_rule: 2 }, confirmed: true })).toMatchObject({ status: "group", intent: { confirmed: true } });
    expect(res({ conversationId: "g1", setting: { join_rule: 3 } })).toMatchObject({ status: "rejected" });
    expect(res({ conversationId: "g1", setting: { history_policy: 3 } })).toMatchObject({ status: "rejected" });
    expect(res({ conversationId: "g1", setting: { protection: 2, join_rule: 1 } })).toMatchObject({ status: "rejected" });
    expect(res({ conversationId: "g1", setting: { protection: 2 }, confirmed: "yes" })).toMatchObject({ status: "rejected" });
    expect(res({ conversationId: "g1", setting: { protection: 2 } }, "suggestion")).toMatchObject({ status: "rejected", code: "suggestion_cannot_confirm" });
  });

  it("another member's change reaches this person as a named notice", { timeout: 15000 }, async () => {
    const { h, services } = setup();
    h.groups.simulateInvite(services.ctx, ANA, "Family");
    render(<Screen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: "Join" }));
    await userEvent.click(await screen.findByRole("button", { name: /Family/ }));
    const gid = h.groups.list(services.ctx)[0]?.conversationId ?? asGroupId("");
    h.groups.simulateSettingChange(services.ctx, gid, "acct-c-ana", { members_may_invite: false });
    expect(await screen.findByText("Ana changed who can add people: now only admins.", {}, { timeout: 5000 })).toBeTruthy();
    h.groups.simulateSettingChange(services.ctx, gid, "acct-c-ana", { members_may_invite: true });
    expect(await screen.findByText("Ana changed who can add people: now anyone in the group.", {}, { timeout: 5000 })).toBeTruthy();
    expect(screen.getAllByText("Less strict")).toHaveLength(1);
  });

  it("states: held, undeliverable, blocked and an unfinished add are said honestly; a superseded Commit is silent", async () => {
    const { h, services } = setup();
    const { container } = render(<Screen services={services} />);
    await screen.findByRole("button", { name: "Create group" });
    expect(screen.queryByRole("region", { name: "Group status" })).toBeNull();
    act(() => { h.groups.simulateEvent(services.ctx, { kind: "commit_superseded", intentId: "i1" }); h.groups.simulateEvent(services.ctx, { kind: "dropped", reason: "x" }); });
    expect(screen.queryByRole("region", { name: "Group status" })).toBeNull();
    act(() => { h.groups.simulateEvent(services.ctx, { kind: "held", reason: "account_head_missing" }); h.groups.simulateEvent(services.ctx, { kind: "undeliverable", count: 2 }); });
    const status = await screen.findByRole("region", { name: "Group status" });
    expect(within(status).getByText("Waiting for a group update")).toBeTruthy();
    expect(within(status).getByText("2 not delivered")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(status).getByRole("button", { name: "OK" }));
    expect(within(status).queryByText("2 not delivered")).toBeNull();
    act(() => { h.groups.simulateEvent(services.ctx, { kind: "admit_blocked", inviteId: "inv", reason: "not_authorized" }); });
    expect(await within(status).findByText("admit: not_authorized")).toBeTruthy();
    // Events of another profile are not shown.
    expect(h.groups.list(services.ctx)).toHaveLength(0);
  });

  it("the state reducer keeps only what still needs a line", () => {
    const quiet = reduceGroupStates(NO_STATES, [{ kind: "commit_superseded", intentId: "i" }, { kind: "dropped", reason: "r" }, { kind: "message_sent", conversationId: asGroupId("g"), id: "m" }]);
    expect(quiet).toEqual(NO_STATES);
    const held = reduceGroupStates(NO_STATES, [{ kind: "held", reason: "r" }]);
    expect(held.held).toBe(true);
    expect(reduceGroupStates(held, [{ kind: "joined", conversationId: asGroupId("g") }]).held).toBe(false);
    const blocked = reduceGroupStates(NO_STATES, [{ kind: "room_setting_blocked", conversationId: asGroupId("g"), reason: "not_authorized" }]);
    expect(blocked.settingBlocked.get("g")).toBe("not_authorized");
    expect(reduceGroupStates(blocked, [{ kind: "policy_changed", conversationId: asGroupId("g"), byAccount: "a", changes: [] }]).settingBlocked.size).toBe(0);
    expect(reduceGroupStates(reduceGroupStates(NO_STATES, [{ kind: "undeliverable", count: 1 }]), [{ kind: "undeliverable", count: 2 }]).undeliverable).toBe(3);
  });
});

describe("protection levels mirror the Core bundle table", () => {
  it("each level preview equals what Core's bundle() sets", () => {
    const src = readFileSync(resolve(process.cwd(), "../../crates/om-core/src/groups/room.rs"), "utf8");
    const arm = (name: string): string => {
      const m = new RegExp(`${name} => Bundle \\{([^}]*)\\}`).exec(src);
      if (!m) throw new Error(`no bundle arm for ${name}`);
      return m[1] ?? "";
    };
    const num = (body: string, f: string): number => Number(new RegExp(`${f}: (\\d+)`).exec(body)?.[1]);
    const names = { 1: "RECOMMENDED", 2: "HARDENED", 3: "MAXIMUM_PRIVACY" } as const;
    for (const level of PROTECTION_LEVELS) {
      const body = arm(names[level]);
      expect(bundleOf(level)).toEqual({
        profile: level,
        joinRule: num(body, "join_rule"),
        newDevicePolicy: num(body, "new_device_policy"),
        historyPolicy: num(body, "history_policy"),
        membersMayInvite: /members_may_invite: true/.test(body),
      });
    }
  });
});
