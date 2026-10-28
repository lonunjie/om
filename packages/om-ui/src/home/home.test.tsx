// @vitest-environment jsdom
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { testHomeCopy } from "./__fixtures__/copy";
import { HomeScreen, type HomeProbeResult, type HomeSettingsServices } from "./index";

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
    expect(screen.getByText("Address: https://old.example")).toBeTruthy();
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
