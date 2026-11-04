// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { testHomeCopy } from "./__fixtures__/copy";
import { HomeScreen, type AddBackupResult, type HomeProbeResult, type HomeSettingsServices, type MoreHomesServices, type MoveResult } from "./index";

afterEach(cleanup);
const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const OK = (over: Partial<Extract<HomeProbeResult, { kind: "ok" }>> = {}): HomeProbeResult => ({ kind: "ok", baseUrl: "https://new.example", providerId: "new-home", health: "healthy", ...over });

function setup(over: Partial<HomeSettingsServices> & { answers?: Record<string, HomeProbeResult> } = {}) {
  const { answers: given = {}, ...rest } = over;
  const answers: Record<string, HomeProbeResult> = { "https://old.example": { kind: "ok", baseUrl: "https://old.example", providerId: "old-home", health: "healthy" }, ...given };
  const saved: string[] = [];
  let lost: readonly string[] = [];
  const services: HomeSettingsServices = {
    current: { baseUrl: "https://old.example", source: "chosen", hasDefault: true },
    probe: vi.fn((input: string) => Promise.resolve(answers[input.trim()] ?? { kind: "unreachable" as const })),
    save: vi.fn((u: string) => { saved.push(u); return Promise.resolve(true); }),
    clear: vi.fn(() => Promise.resolve(true)),
    lostMailboxes: () => lost,
    reconnect: vi.fn(),
    ...rest,
  };
  return { services, saved, setLost: (l: readonly string[]) => { lost = l; } };
}

const check = async (address: string): Promise<void> => {
  await userEvent.type(screen.getByLabelText("Home address"), address);
  await userEvent.click(screen.getByRole("button", { name: "Check this Home" }));
};

describe("Home settings screen", () => {
  it("shows the Home in use and what it says about itself, honestly labelled; accessible", async () => {
    const { services } = setup({ answers: { "https://old.example": { kind: "ok", baseUrl: "https://old.example", providerId: "old-home", health: "healthy" } } });
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
    expect(screen.getByRole("heading", { level: 1, name: "Your Home" })).toBeTruthy();
    expect(await screen.findByText(/Reachable: old-home\. Healthy\. \(says the Home itself\)/)).toBeTruthy();
    expect(screen.getByText("Address: https://old.example").hidden).toBe(true); // behind a disclosure
    await userEvent.click(screen.getByRole("button", { name: "Show address" }));
    expect(screen.getByText("Address: https://old.example").hidden).toBe(false);
    expect(screen.getByRole("button", { name: "Hide address" }).getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("You chose this Home")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("nothing can be chosen before the address has been checked; a healthy Home shows its id and is then chosen", async () => {
    const { services, saved } = setup({ answers: { "https://new.example": OK() } });
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
    expect(screen.queryByRole("button", { name: /Use this Home|Use it anyway/ })).toBeNull();
    await check("https://new.example");
    expect(await screen.findByText(/Reachable: new-home\. Healthy\./)).toBeTruthy();
    expect(screen.getByText("Existing conversations may stop working")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: "Use this Home" }));
    expect(saved).toEqual(["https://new.example"]);
    expect(await screen.findByText("Saved")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Reconnect now" }));
    expect(services.reconnect).toHaveBeenCalledTimes(1);
  });

  it("editing the address after a check withdraws the offer, so only a checked address can be saved", async () => {
    const { services, saved } = setup({ answers: { "https://new.example": OK() } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await check("https://new.example");
    await screen.findByRole("button", { name: "Use this Home" });
    await userEvent.type(screen.getByLabelText("Home address"), "x");
    expect(screen.queryByRole("button", { name: "Use this Home" })).toBeNull();
    expect(saved).toEqual([]);
  });

  it("a degraded or unknown Home is shown as such and needs the explicit 'anyway'; an unhealthy one cannot be chosen", async () => {
    for (const [health, expectBtn] of [["degraded", "Use it anyway"], ["unknown", "Use it anyway"], ["unhealthy", null]] as const) {
      cleanup();
      const { services } = setup({ answers: { "https://new.example": OK({ health }) } });
      const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
      await check("https://new.example");
      const alert = await screen.findByRole("alert");
      expect(alert.textContent).toContain(health === "unknown" ? "Health unknown." : `${health[0]?.toUpperCase() ?? ""}${health.slice(1)}.`);
      if (expectBtn === null) expect(screen.queryByRole("button", { name: /Use this Home|Use it anyway/ })).toBeNull();
      else expect(screen.getByRole("button", { name: expectBtn })).toBeTruthy();
      expect(await axeViolations(container)).toEqual([]);
    }
  });

  it("says why an address cannot be used: unreachable, server error, not a Home, wrong version, no mailbox, and every URL problem; none can be saved", async () => {
    const cases: [HomeProbeResult, RegExp][] = [
      [{ kind: "unreachable" }, /Could not reach it/],
      [{ kind: "failing", status: 503 }, /Server error 503/],
      [{ kind: "not_a_home" }, /Not a Home/],
      [{ kind: "wrong_version", offered: ["2", "3"] }, /Wrong version: 2, 3/],
      [{ kind: "no_mailbox" }, /No mailboxes/],
      [{ kind: "invalid_url", reason: "not_https" }, /Must be https/],
      [{ kind: "invalid_url", reason: "credentials" }, /No passwords/],
      [{ kind: "invalid_url", reason: "query_or_fragment" }, /No extra parts/],
      [{ kind: "invalid_url", reason: "malformed" }, /Not an address/],
    ];
    for (const [answer, text] of cases) {
      cleanup();
      const { services, saved } = setup({ answers: { "x": answer } });
      const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
      await check("x");
      expect((await screen.findByRole("alert")).textContent).toMatch(text);
      expect(screen.queryByRole("button", { name: /Use this Home|Use it anyway/ })).toBeNull();
      expect(saved).toEqual([]);
      expect(await axeViolations(container)).toEqual([]);
    }
  });

  it("shows the Home in use turning out unreachable, wrong version or degraded", async () => {
    for (const [answer, text] of [
      [{ kind: "unreachable" }, /Could not reach it/],
      [{ kind: "wrong_version", offered: ["9"] }, /Wrong version: 9/],
      [{ kind: "ok", baseUrl: "https://old.example", providerId: "old-home", health: "degraded" }, /Degraded\./],
    ] as [HomeProbeResult, RegExp][]) {
      cleanup();
      render(<HomeScreen services={setup({ answers: { "https://old.example": answer } }).services} copy={testHomeCopy} />);
      expect((await screen.findByRole("alert")).textContent).toMatch(text);
    }
  });

  it("a probe that throws is reported as unreachable, never as success", async () => {
    const { services } = setup({ probe: () => Promise.reject(new Error("boom")) });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await check("https://new.example");
    await waitFor(() => { expect(screen.getAllByRole("alert").length).toBeGreaterThan(0); });
    expect(screen.queryByRole("button", { name: /Use this Home|Use it anyway/ })).toBeNull();
  });

  it("a refused save is reported and nothing is offered as done", async () => {
    const { services } = setup({ answers: { "https://new.example": OK() }, save: () => Promise.resolve(false) });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await check("https://new.example");
    await userEvent.click(await screen.findByRole("button", { name: "Use this Home" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Could not save");
    expect(screen.queryByRole("button", { name: "Reconnect now" })).toBeNull();
  });

  it("choosing the Home already in use offers no save", async () => {
    const { services } = setup({ answers: { "https://old.example": { kind: "ok", baseUrl: "https://old.example", providerId: "old-home", health: "healthy" } } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await check("https://old.example");
    expect(await screen.findByText("Already in use")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Use this Home" })).toBeNull();
  });

  it("with no Home it says nothing leaves the device; with a chosen Home it can go back to the default or disconnect", async () => {
    const none = setup({ current: { baseUrl: undefined, source: "none", hasDefault: false } });
    const { container } = render(<HomeScreen services={none.services} copy={testHomeCopy} />);
    expect(screen.getByText("No Home is set up")).toBeTruthy();
    expect(none.services.probe).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: /default|Disconnect/ })).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
    cleanup();

    const withDefault = setup();
    render(<HomeScreen services={withDefault.services} copy={testHomeCopy} />);
    await userEvent.click(screen.getByRole("button", { name: "Use the default Home" }));
    expect(withDefault.services.clear).toHaveBeenCalled();
    expect(await screen.findByText("Choice removed")).toBeTruthy();
    cleanup();

    const noDefault = setup({ current: { baseUrl: "https://old.example", source: "chosen", hasDefault: false } });
    render(<HomeScreen services={noDefault.services} copy={testHomeCopy} />);
    expect(screen.getByRole("button", { name: "Disconnect" })).toBeTruthy();
    cleanup();

    const built = setup({ current: { baseUrl: "https://old.example", source: "default", hasDefault: true } });
    render(<HomeScreen services={built.services} copy={testHomeCopy} />);
    expect(screen.getByText("This app's default Home")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Use the default Home" })).toBeNull();
  });

  it("surfaces lost mailboxes (shortened references only) and updates when one is lost while the screen is open", async () => {
    const { services, setLost } = setup();
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} refreshMs={20} />);
    expect(screen.queryByText("Mailboxes held by someone else")).toBeNull();
    await act(async () => { setLost(["0123456789abcdef0123456789abcdef", "fedcba9876543210fedcba9876543210"]); await new Promise((r) => setTimeout(r, 60)); });
    expect(await screen.findByRole("heading", { name: "Mailboxes held by someone else" })).toBeTruthy();
    expect(screen.getByText("2 mailbox(es) cannot be used")).toBeTruthy();
    expect(screen.getByText("Reference 01234567")).toBeTruthy();
    expect(screen.queryByText(/0123456789abcdef0123/)).toBeNull(); // the full id is never shown
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("Advanced: several delivery services", () => {
  const MAIN = "https://old.example";
  const BACKUP = "https://backup.example";
  const HEX = "cd".repeat(32);
  const locked = (baseUrl: string): HomeProbeResult => ({ kind: "ok", baseUrl, providerId: "mine", health: "healthy", admission: true });

  function several(over: { services?: MoreHomesServices["services"]; add?: AddBackupResult; move?: MoveResult; answers?: Record<string, HomeProbeResult>; deployUrl?: string | undefined } = {}) {
    const add = vi.fn((): Promise<AddBackupResult> => Promise.resolve(over.add ?? "added"));
    const move = vi.fn((): Promise<MoveResult> => Promise.resolve(over.move ?? { kind: "moved", stuck: 0 }));
    const prepare = vi.fn(() => Promise.resolve(HEX));
    const more: MoreHomesServices = {
      services: over.services ?? [{ baseUrl: MAIN, role: "main" }, { baseUrl: BACKUP, role: "backup" }],
      setup: { deployUrl: "deployUrl" in over ? over.deployUrl : "https://deploy.example/start", prepare },
      add, move,
    };
    const { services } = setup({ more, answers: { [BACKUP]: { kind: "ok", baseUrl: BACKUP, providerId: "backup-home", health: "degraded" }, ...(over.answers ?? {}) } });
    return { services, add, move, prepare };
  }
  const open = (): Promise<void> => userEvent.click(screen.getByRole("button", { name: "Advanced" }));

  it("is absent when the host holds only one delivery service", () => {
    const { services } = setup();
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    expect(screen.queryByRole("button", { name: "Advanced" })).toBeNull();
  });

  it("is a closed disclosure; nothing but the service in use is asked about until it is opened", async () => {
    const { services } = several();
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
    expect(screen.getByRole("button", { name: "Advanced" }).getAttribute("aria-expanded")).toBe("false");
    await screen.findByText(/Reachable: old-home/);
    expect(vi.mocked(services.probe).mock.calls.map((c) => c[0])).toEqual([MAIN]);
    await open();
    expect(screen.getByRole("button", { name: "Advanced" }).getAttribute("aria-expanded")).toBe("true");
    await waitFor(() => { expect(vi.mocked(services.probe).mock.calls.map((c) => c[0])).toContain(BACKUP); });
    expect(await axeViolations(container)).toEqual([]);
  });

  it("lists every service with its role and how it is doing; the address stays behind a disclosure per service", async () => {
    const { services } = several({ services: [{ baseUrl: MAIN, role: "main" }, { baseUrl: BACKUP, role: "backup" }, { baseUrl: "https://gone.example", role: "leaving" }], answers: { "https://gone.example": { kind: "unreachable" } } });
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(within(rows[0] as HTMLElement).getByRole("heading", { name: "Main" })).toBeTruthy();
    expect(await within(rows[1] as HTMLElement).findByText(/Reachable: backup-home\. Degraded\./)).toBeTruthy();
    expect(within(rows[1] as HTMLElement).getByRole("alert")).toBeTruthy(); // a service that is not healthy is announced
    expect(within(rows[2] as HTMLElement).getByRole("heading", { name: "Moving away" })).toBeTruthy();
    expect(await within(rows[2] as HTMLElement).findByText("Could not reach it")).toBeTruthy();
    const second = rows[1] as HTMLElement;
    expect(within(second).getByText(`Address: ${BACKUP}`).hidden).toBe(true);
    await userEvent.click(within(second).getByRole("button", { name: "Show address" }));
    expect(within(second).getByText(`Address: ${BACKUP}`).hidden).toBe(false);
    expect(await axeViolations(container)).toEqual([]);
  });

  it("with a backup, the single-service replace and forget controls are not offered (they would drop more than the person sees)", () => {
    const { services } = several();
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    expect(screen.queryByRole("heading", { name: "Choose a different Home" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Use the default Home" })).toBeNull();
    const one = setup({ more: { services: [{ baseUrl: MAIN, role: "main" }], setup: { deployUrl: undefined, prepare: () => Promise.resolve(HEX) }, add: () => Promise.resolve("added"), move: () => Promise.resolve({ kind: "last" }) } });
    cleanup();
    render(<HomeScreen services={one.services} copy={testHomeCopy} />);
    expect(screen.getByRole("heading", { name: "Choose a different Home" })).toBeTruthy();
  });

  it("add a backup: shows the setup value, the setup link in a new tab, and copies the value", async () => {
    const { services, prepare } = several({ services: [{ baseUrl: MAIN, role: "main" }] });
    const written: string[] = [];
    const copyText = vi.fn((t: string) => { written.push(t); return Promise.resolve(true); });
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} copyText={copyText} />);
    await open();
    expect(prepare).not.toHaveBeenCalled(); // nothing is made until the person asks to add one
    await userEvent.click(screen.getByRole("button", { name: "Add a backup" }));
    expect(await screen.findByText(HEX)).toBeTruthy();
    const link = screen.getByRole("link", { name: /Open Cloudflare setup/ });
    expect(link.getAttribute("href")).toBe("https://deploy.example/start");
    expect(link.getAttribute("target")).toBe("_blank");
    expect(link.getAttribute("rel")).toContain("noopener");
    await userEvent.click(screen.getByRole("button", { name: "Copy setup value" }));
    expect(written).toEqual([HEX]);
    expect(await screen.findByText("Setup value copied")).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("add a backup: without a clipboard there is no copy button, without a setup link it says so", async () => {
    const { services } = several({ services: [{ baseUrl: MAIN, role: "main" }], deployUrl: undefined });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Add a backup" }));
    await screen.findByText(HEX);
    expect(screen.queryByRole("button", { name: "Copy setup value" })).toBeNull();
    expect(screen.queryByRole("link", { name: /Open Cloudflare setup/ })).toBeNull();
    expect(screen.getByText("Not ready yet.")).toBeTruthy();
  });

  const typeAddress = async (value: string): Promise<void> => {
    await userEvent.type(screen.getByLabelText("Address of your delivery service"), value);
    await userEvent.click(screen.getByRole("button", { name: "Check and add" }));
  };

  it("add a backup: only an address that answers as a delivery service that asks for the setup value is stored", async () => {
    const { services, add } = several({ services: [{ baseUrl: MAIN, role: "main" }], answers: {
      "https://open.example": { kind: "ok", baseUrl: "https://open.example", providerId: "open", health: "healthy", admission: false },
      "https://down.example": { kind: "ok", baseUrl: "https://down.example", providerId: "down", health: "unhealthy", admission: true },
      "https://nothome.example": { kind: "not_a_home" },
    } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Add a backup" }));
    await screen.findByText(HEX);
    await typeAddress("https://open.example");
    expect(await screen.findByText("Not locked")).toBeTruthy(); // open to anyone who learns the address
    await userEvent.clear(screen.getByLabelText("Address of your delivery service"));
    await typeAddress("https://down.example");
    expect(await screen.findByText(/Unhealthy\./)).toBeTruthy();
    await userEvent.clear(screen.getByLabelText("Address of your delivery service"));
    await typeAddress("https://nothome.example");
    expect(await screen.findByText("Not a Home")).toBeTruthy();
    await userEvent.clear(screen.getByLabelText("Address of your delivery service"));
    await typeAddress("nope");
    expect(await screen.findByText("Could not reach it")).toBeTruthy();
    expect(add).not.toHaveBeenCalled();
  });

  it("add a backup: a stored address says to connect again; refusals are said in plain words and nothing is claimed", async () => {
    const good = locked("https://second.example");
    const { services, add } = several({ services: [{ baseUrl: MAIN, role: "main" }], answers: { "https://second.example": good } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Add a backup" }));
    await screen.findByText(HEX);
    add.mockResolvedValueOnce("already");
    await typeAddress("https://second.example");
    expect(await screen.findByText("Already in your list")).toBeTruthy();
    add.mockResolvedValueOnce("full");
    await userEvent.click(screen.getByRole("button", { name: "Check and add" }));
    expect(await screen.findByText("No room for more")).toBeTruthy();
    add.mockResolvedValueOnce("failed");
    await userEvent.click(screen.getByRole("button", { name: "Check and add" }));
    expect(await screen.findByText("Not saved")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Reconnect now" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Check and add" }));
    expect(await screen.findByText("Added as a backup")).toBeTruthy();
    expect(add).toHaveBeenLastCalledWith("https://second.example");
    await userEvent.click(screen.getByRole("button", { name: "Reconnect now" }));
    expect(services.reconnect).toHaveBeenCalledTimes(1);
  });

  it("move away: asks first, says what happens, then moves, shows the service as moving away and offers to connect", async () => {
    const { services, move } = several();
    const { container } = render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    const [first] = screen.getAllByRole("listitem") as [HTMLElement];
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    expect(move).not.toHaveBeenCalled();
    const ask = within(first).getByRole("group", { name: "Move away?" });
    expect(within(ask).getByText("Your chats switch.")).toBeTruthy();
    expect(within(ask).getByText("Unaccepted invitations stop working.")).toBeTruthy(); // leaving the main service orphans open invitations
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(within(ask).getByRole("button", { name: "Keep using it" }));
    expect(move).not.toHaveBeenCalled();
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    await userEvent.click(within(first).getByRole("button", { name: "Move away" }));
    expect(move).toHaveBeenCalledWith(MAIN);
    expect(await screen.findByText("Moving. Connect again to finish.")).toBeTruthy();
    expect(within(first).getByRole("heading", { name: "Moving away" })).toBeTruthy();
    expect(within(first).getByRole("button", { name: "Try moving again" })).toBeTruthy();
    // Only one service is left that is not being left: the backup cannot be moved away from.
    const second = screen.getAllByRole("listitem")[1] as HTMLElement;
    expect(within(second).queryByRole("button", { name: "Move away from this one" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Reconnect now" }));
    expect(services.reconnect).toHaveBeenCalledTimes(1);
  });

  it("move away: chats that have no other service are named as a plain refusal and the move can be tried again", async () => {
    const { services, move } = several({ move: { kind: "moved", stuck: 3 } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    const [first] = screen.getAllByRole("listitem") as [HTMLElement];
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    await userEvent.click(within(first).getByRole("button", { name: "Move away" }));
    const note = await within(first).findByRole("alert");
    expect(note.textContent).toBe("3 chat(s) cannot move yet");
    move.mockResolvedValueOnce({ kind: "moved", stuck: 0 });
    await userEvent.click(within(first).getByRole("button", { name: "Try moving again" }));
    await waitFor(() => { expect(within(first).queryByText("3 chat(s) cannot move yet")).toBeNull(); });
    expect(move).toHaveBeenCalledTimes(2);
  });

  it("move away: a failure changes nothing on screen but says so; the last service says to add a backup first", async () => {
    const { services, move } = several({ move: { kind: "failed" } });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    const [first] = screen.getAllByRole("listitem") as [HTMLElement];
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    await userEvent.click(within(first).getByRole("button", { name: "Move away" }));
    expect(await within(first).findByText("Move failed")).toBeTruthy();
    expect(within(first).getByRole("heading", { name: "Main" })).toBeTruthy();
    move.mockResolvedValueOnce({ kind: "last" });
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    await userEvent.click(within(first).getByRole("button", { name: "Move away" }));
    expect(await within(first).findByText("Add a backup first")).toBeTruthy();
    move.mockRejectedValueOnce(new Error("x"));
    await userEvent.click(within(first).getByRole("button", { name: "Move away from this one" }));
    await userEvent.click(within(first).getByRole("button", { name: "Move away" }));
    expect(await within(first).findByText("Move failed")).toBeTruthy();
  });

  it("with only one service there is nothing to move to: no move button, and the way forward is said", async () => {
    const { services } = several({ services: [{ baseUrl: MAIN, role: "main" }] });
    render(<HomeScreen services={services} copy={testHomeCopy} />);
    await open();
    expect(screen.queryByRole("button", { name: "Move away from this one" })).toBeNull();
    expect(screen.getByText("Add a backup first")).toBeTruthy();
  });
});
