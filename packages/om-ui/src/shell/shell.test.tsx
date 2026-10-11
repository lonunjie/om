// @vitest-environment jsdom
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { cleanup, render, renderHook, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement, ReactNode } from "react";
import axe from "axe-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BREAKPOINTS } from "@om/design";
import { App } from "../App";
import {
  AppShell, Button, IconButton, LiveRegion, SecurityNoticeBanner, SkipLink, Text, ThemeProvider, VisuallyHidden,
  useReducedMotion, type ShellDestination,
} from "./index";

// Minimal matchMedia: width-based min-width queries plus boolean flags for other queries.
const env = { width: 400, flags: new Set<string>() };
const listeners = new Set<() => void>();
const matches = (q: string): boolean => {
  const m = /\(min-width:\s*(\d+)px\)/.exec(q);
  return m ? env.width >= Number(m[1]) : env.flags.has(q);
};
const setEnv = (width: number, flags: string[] = []): void => { env.width = width; env.flags = new Set(flags); };

beforeEach(() => {
  setEnv(400);
  vi.stubGlobal("matchMedia", (q: string) => ({
    get matches() { return matches(q); },
    media: q,
    addEventListener: (_: string, l: () => void) => { listeners.add(l); },
    removeEventListener: (_: string, l: () => void) => { listeners.delete(l); },
  }));
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); document.documentElement.removeAttribute("style"); });

const axeViolations = async (el: Element): Promise<string[]> => {
  const r = await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  return r.violations.map((v) => `${v.id}: ${v.help}`);
};

const ONE: readonly ShellDestination[] = [{ id: "chats", label: "Chats" }];
const TWO: readonly ShellDestination[] = [...ONE, { id: "people", label: "People" }];
const labels = { navigation: "Main", list: "Chat list", detail: "Conversation", skipToContent: "Skip" };

const shell = (destinations: readonly ShellDestination[], open = false, extra: Partial<Parameters<typeof AppShell>[0]> = {}) => (
  <AppShell
    destinations={destinations} activeDestination="chats" onDestinationChange={() => undefined} conversationOpen={open}
    labels={labels} list={<button type="button">list item</button>} detail={<button type="button">detail item</button>}
    composerZone={<input aria-label="Message" />} omnibarTrigger={<button type="button">find</button>}
    profileControl={<button type="button">profile</button>} {...extra}
  />
);

describe("AppShell", () => {
  it("compact with one destination renders no tablist", () => {
    render(shell(ONE));
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByRole("region", { name: "Chat list" })).toBeTruthy();
  });
  it("compact with two destinations renders a tablist", () => {
    render(shell(TWO));
    const tabs = screen.getByRole("tablist", { name: "Main" });
    expect(within(tabs).getAllByRole("tab")).toHaveLength(2);
  });
  it("compact conversation hides tabs, list, and puts the omnibar trigger in the composer zone", () => {
    const { container } = render(shell(TWO, true));
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.queryByRole("region", { name: "Chat list" })).toBeNull();
    const composer = container.querySelector(".om-shell__composer");
    expect(within(composer as HTMLElement).getByText("find")).toBeTruthy();
    expect(screen.queryByText("profile")).toBeNull();
  });
  it("arrow keys move between tabs", async () => {
    const onChange = vi.fn();
    render(shell(TWO, false, { onDestinationChange: onChange }));
    screen.getByRole("tab", { name: "Chats" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("people");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "People" }));
  });
  it("expanded shows a rail (no tablist) with both panes, trigger and profile", () => {
    setEnv(BREAKPOINTS.expanded);
    const { container } = render(shell(ONE, false));
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByRole("navigation", { name: "Main" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Chat list" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Conversation" })).toBeTruthy();
    expect(container.querySelector(".om-shell__rail-footer")?.textContent).toBe("profile");
    expect(container.querySelector(".om-shell__topbar")?.textContent).toBe("find");
  });
  it("split uses panes plus the tab bar", () => {
    setEnv(BREAKPOINTS.split);
    render(shell(TWO));
    expect(screen.getByRole("tablist")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Conversation" })).toBeTruthy();
  });
  it("focus order is list -> detail -> composer", async () => {
    setEnv(BREAKPOINTS.expanded);
    render(shell(ONE));
    const order: string[] = [];
    for (let i = 0; i < 8; i++) {
      await userEvent.tab();
      const el = document.activeElement;
      order.push(el?.getAttribute("aria-label") ?? el?.textContent ?? "");
    }
    const at = (s: string): number => order.indexOf(s);
    expect(at("list item")).toBeGreaterThanOrEqual(0);
    expect(at("list item")).toBeLessThan(at("detail item"));
    expect(at("detail item")).toBeLessThan(at("Message"));
  });
  it("mirrored flag is reflected for CSS", () => {
    const { container } = render(shell(ONE, false, { mirrored: true }));
    expect(container.querySelector(".om-shell")?.getAttribute("data-mirrored")).toBe("true");
  });
  it.each([[400, ONE, false], [400, TWO, false], [400, TWO, true], [700, TWO, false], [1000, TWO, false]] as const)(
    "axe: width %i, %i destinations, open=%s has no violations", async (w, dests, open) => {
      setEnv(w);
      const { container } = render(shell(dests, open));
      expect(await axeViolations(container)).toEqual([]);
    });
  it("App renders and has no violations", async () => {
    const { container } = render(<App />);
    expect(await axeViolations(container)).toEqual([]);
    expect(screen.queryByRole("tablist")).toBeNull();
  });
});

describe("primitives", () => {
  const cases: [string, () => ReactElement][] = [
    ["Button", () => <Button variant="primary">Go</Button>],
    ["IconButton", () => <IconButton label="Close" icon="x" />],
    ["Text", () => <Text as="h2" size="lg" tone="muted">Hello</Text>],
    ["VisuallyHidden", () => <VisuallyHidden>hidden</VisuallyHidden>],
    ["LiveRegion", () => <LiveRegion>Saved</LiveRegion>],
    ["SkipLink", () => <><SkipLink targetId="t">Skip</SkipLink><main id="t">x</main></>],
    ["SecurityNoticeBanner", () => <SecurityNoticeBanner reasonCode="identity_changed" label="Changed" tone="danger" />],
  ];
  for (const [name, make] of cases) {
    it(`${name}: stable DOM structure and no axe violations`, async () => {
      const { container } = render(make());
      expect(container.innerHTML).toMatchSnapshot();
      expect(await axeViolations(container)).toEqual([]);
    });
  }
  it("IconButton exposes its label as the accessible name", () => {
    render(<IconButton label="Close" icon="x" />);
    expect(screen.getByRole("button", { name: "Close" })).toBeTruthy();
  });
  it("SkipLink moves focus to the target", async () => {
    render(<><SkipLink targetId="t">Skip</SkipLink><main id="t">x</main></>);
    await userEvent.click(screen.getByText("Skip"));
    expect(document.activeElement?.id).toBe("t");
  });
  it("SecurityNoticeBanner carries the machine code and no built-in copy", () => {
    const { container } = render(<SecurityNoticeBanner reasonCode="bridge_active" label="Localized" />);
    expect(container.querySelector("[data-reason-code='bridge_active']")?.textContent).toBe("Localized");
  });
});

describe("CSS contracts", () => {
  const css = (rel: string): string => readFileSync(new URL(rel, import.meta.url), "utf8");
  it("shell.css media queries use only the shared breakpoints", () => {
    const found = [...css("./layout/shell.css").matchAll(/min-width:\s*(\d+)px/g)].map((m) => Number(m[1]));
    expect(new Set(found)).toEqual(new Set(Object.values(BREAKPOINTS)));
  });
  it("security notice CSS uses only reserved color tokens", () => {
    const vars = [...css("./theme/security-notice.css").matchAll(/var\(--om-([a-z-]+)\)/g)].map((m) => m[1] ?? "");
    expect(vars.length).toBeGreaterThan(0);
    expect(vars.filter((v) => !v.startsWith("color-security-"))).toEqual([]);
  });
  it("no inline <style>/style attributes in shell sources", () => {
    for (const f of ["./layout/AppShell.tsx", "./theme/ThemeProvider.tsx", "./theme/SecurityNoticeBanner.tsx"]) {
      expect(css(f)).not.toMatch(/<style|style=\{/);
    }
  });
  it("primitives enforce a 44px minimum target and a visible focus ring", () => {
    const p = css("./primitives/primitives.css");
    expect(p).toMatch(/min-block-size:\s*var\(--om-target-min\)/);
    expect(p).toMatch(/:focus-visible/);
  });
});

describe("theme and motion", () => {
  const wrap = (motion: "system" | "reduced" | "full") => ({ children }: { children: ReactNode }) =>
    <ThemeProvider motion={motion}>{children}</ThemeProvider>;
  it("useReducedMotion follows the system preference, overridden by OM preference", () => {
    setEnv(400, ["(prefers-reduced-motion: reduce)"]);
    expect(renderHook(() => useReducedMotion(), { wrapper: wrap("system") }).result.current).toBe(true);
    expect(renderHook(() => useReducedMotion(), { wrapper: wrap("full") }).result.current).toBe(false);
    setEnv(400);
    expect(renderHook(() => useReducedMotion(), { wrapper: wrap("system") }).result.current).toBe(false);
    expect(renderHook(() => useReducedMotion(), { wrapper: wrap("reduced") }).result.current).toBe(true);
  });
  it("ThemeProvider toggles root attributes and falls back from an invalid theme", () => {
    const bad = { id: "evil", name: "Evil", light: {}, dark: {} };
    render(<ThemeProvider appearance="dark" motion="reduced" theme={bad as never}><div /></ThemeProvider>);
    const root = document.documentElement;
    expect(root.getAttribute("data-theme")).toBe("om-calm");
    expect(root.getAttribute("data-appearance")).toBe("dark");
    expect(root.getAttribute("data-motion")).toBe("reduced");
    expect(root.getAttribute("style")).toBeNull();
  });
});
