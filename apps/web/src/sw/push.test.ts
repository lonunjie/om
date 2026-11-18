// Service-worker side of the push wake (ADR-0210): an empty push becomes one generic notice, or an opaque wake of a window that is open.
// No fetch, no vault, no capability here: the worker never reads anything but its own two strings.
import { describe, expect, it } from "vitest";
import { DEFAULT_NOTICE, WAKE_MESSAGE, NOTICE_KEY, noticeFrom, onNotificationClick, onPush, type PushDeps, type WindowLike } from "./push";

const win = (visible: boolean, focused = false): WindowLike & { posted: unknown[]; focusedNow: boolean } => {
  const w = { visible, posted: [] as unknown[], focusedNow: false, post: (m: unknown) => { w.posted.push(m); }, focus: () => { w.focusedNow = true; return Promise.resolve(); } };
  return Object.assign(w, { focused });
};
function deps(windows: ReturnType<typeof win>[], stored: unknown = null) {
  const shown: { title: string; body: string; tag: string }[] = [];
  const opened: string[] = [];
  const d: PushDeps = {
    windows: () => Promise.resolve(windows),
    show: (title, o) => { shown.push({ title, ...o }); return Promise.resolve(); },
    read: (k) => Promise.resolve(k === NOTICE_KEY ? (stored as string | null) : null),
    open: (url) => { opened.push(url); return Promise.resolve(); },
  };
  return { d, shown, opened };
}

describe("onPush", () => {
  it("with no window open shows one generic notice and nothing else", async () => {
    const { d, shown } = deps([]);
    await onPush(d);
    expect(shown).toEqual([{ title: DEFAULT_NOTICE.title, body: DEFAULT_NOTICE.body, tag: "om-wake" }]);
    expect(DEFAULT_NOTICE.body).toBe("New message");
  });
  it("tells every open window to look (opaque, no content) and shows no notice when one is visible", async () => {
    const a = win(true); const b = win(false);
    const { d, shown } = deps([a, b]);
    await onPush(d);
    expect(shown).toEqual([]);
    expect(a.posted).toEqual([WAKE_MESSAGE]);
    expect(b.posted).toEqual([WAKE_MESSAGE]);
    expect(JSON.stringify(WAKE_MESSAGE)).toBe('{"type":"om.sw/wake"}');
  });
  it("shows the notice and still wakes a hidden window", async () => {
    const h = win(false);
    const { d, shown } = deps([h]);
    await onPush(d);
    expect(shown).toHaveLength(1);
    expect(h.posted).toEqual([WAKE_MESSAGE]);
  });
  it("uses the person's language when the page stored it, and the default for anything malformed", async () => {
    const a = deps([], JSON.stringify({ title: "OM", body: "Nuevo mensaje" }));
    await onPush(a.d);
    expect(a.shown[0]?.body).toBe("Nuevo mensaje");
    for (const bad of [null, "", "{", "[]", JSON.stringify({ title: 1, body: "x" }), JSON.stringify({ title: "x".repeat(200), body: "y" }), JSON.stringify({ title: "a", body: "b", extra: 1 })]) {
      expect(noticeFrom(bad), String(bad)).toEqual(DEFAULT_NOTICE);
    }
  });
  it("a failure to read or to wake a window never prevents the notice", async () => {
    const bad: PushDeps = { windows: () => Promise.reject(new Error("x")), show: (t, o) => { shown.push(`${t}${o.body}`); return Promise.resolve(); }, read: () => Promise.reject(new Error("y")), open: () => Promise.resolve() };
    const shown: string[] = [];
    await onPush(bad);
    expect(shown).toEqual(["OMNew message"]);
  });
});

describe("onNotificationClick", () => {
  it("focuses an open window, else opens the app at its root", async () => {
    const a = win(false);
    const one = deps([a]);
    await onNotificationClick(one.d);
    expect(a.focusedNow).toBe(true);
    expect(one.opened).toEqual([]);
    const none = deps([]);
    await onNotificationClick(none.d);
    expect(none.opened).toEqual(["/"]);
  });
});
