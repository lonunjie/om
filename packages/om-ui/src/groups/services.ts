import type { GroupApi, GroupIntent, GroupStep } from "@om/platform-api";
import type { ConversationServices } from "../conversations/services";

/** The groups path of this host, or null when it has none (an unsupported build reports `available() === false`): then no group control is shown at all. */
export function groupApiOf(s: ConversationServices): GroupApi | null {
  const g = s.client.groups;
  return g !== undefined && g.available() ? g : null;
}

export type GroupRun = { readonly ok: true; readonly step: Extract<GroupStep, { ok: true }> } | { readonly ok: false; readonly refused: string };

/**
 * One typed group step, always through the Action Registry (direct origin: a person's own tap) and then `client.groups.do` with the
 * profile context captured when the screen was built (UI-10): a profile switch in between makes Core refuse, never retarget.
 * The step is durable when this resolves. Carrying its `out` is the host's job; the UI never reads or sends a wire.
 */
export async function runGroupAction(s: ConversationServices, actionId: string, params: Readonly<Record<string, unknown>>): Promise<GroupRun> {
  const api = groupApiOf(s);
  if (api === null) return { ok: false, refused: "groups_unavailable" };
  const r = s.registry.resolve({ actionId, params, ctx: s.ctx, origin: "direct" });
  if (r.status !== "group") return { ok: false, refused: "invalid" };
  const intent: GroupIntent = r.intent;
  try {
    const step = await api.do(s.ctx, intent);
    return step.ok ? { ok: true, step } : { ok: false, refused: step.refused };
  } catch {
    return { ok: false, refused: "unavailable" };
  }
}
