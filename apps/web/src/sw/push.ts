// Service-worker side of the push wake (ADR-0210). Pure logic over injected effects so it is testable without a worker. A push carries no data, so there
// is nothing to read: the worker tells any open OM window to look (an opaque message), and unless a window is on screen it shows ONE generic notice
// ("New message"). It never opens the vault, derives a capability or fetches: the page does that, when it is opened or already open.

export const NOTICE_KEY = "push-notice";
export const WAKE_MESSAGE = { type: "om.sw/wake" } as const;
export const NOTICE_TAG = "om-wake";
/** English fallback; the page stores the person's language under NOTICE_KEY when alerts are turned on. */
export const DEFAULT_NOTICE = { title: "OM", body: "New message" } as const;

export interface WindowLike {
  readonly visible: boolean;
  post(message: unknown): void;
  focus(): Promise<unknown>;
}

export interface PushDeps {
  windows(): Promise<readonly WindowLike[]>;
  show(title: string, options: { readonly body: string; readonly tag: string }): Promise<void>;
  read(key: string): Promise<string | null>;
  open(url: string): Promise<unknown>;
}

/** The stored notice if it is exactly two short strings; otherwise the default. Nothing else the page stored is ever shown. */
export function noticeFrom(raw: string | null): { readonly title: string; readonly body: string } {
  if (raw === null || raw === "") return DEFAULT_NOTICE;
  try {
    const v = JSON.parse(raw) as unknown;
    if (typeof v !== "object" || v === null || Array.isArray(v)) return DEFAULT_NOTICE;
    const o = v as Record<string, unknown>;
    if (Object.keys(o).length !== 2 || typeof o["title"] !== "string" || typeof o["body"] !== "string") return DEFAULT_NOTICE;
    if (o["title"].length === 0 || o["title"].length > 60 || o["body"].length === 0 || o["body"].length > 100) return DEFAULT_NOTICE;
    return { title: o["title"], body: o["body"] };
  } catch { return DEFAULT_NOTICE; }
}

export async function onPush(deps: PushDeps): Promise<void> {
  let windows: readonly WindowLike[] = [];
  try { windows = await deps.windows(); } catch { /* no windows known: the notice below still shows */ }
  for (const w of windows) { try { w.post(WAKE_MESSAGE); } catch { /* a closing window */ } }
  // A browser may revoke a subscription whose pushes show nothing, so a notice is shown unless the person is looking at OM right now.
  if (windows.some((w) => w.visible)) return;
  let raw: string | null = null;
  try { raw = await deps.read(NOTICE_KEY); } catch { /* default */ }
  const n = noticeFrom(raw);
  await deps.show(n.title, { body: n.body, tag: NOTICE_TAG });
}

export async function onNotificationClick(deps: Pick<PushDeps, "windows" | "open">): Promise<void> {
  let windows: readonly WindowLike[] = [];
  try { windows = await deps.windows(); } catch { /* open a new one */ }
  const w = windows[0];
  if (w !== undefined) { await w.focus(); return; }
  await deps.open("/");
}
