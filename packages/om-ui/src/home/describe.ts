import type { HomeCopy } from "./copy";
import type { HomeProbeResult } from "./services";

/** Wording for everything a probe can say about an address (the same lines serve the service in use, a candidate and each service in the list). */
export function describeProbe(copy: HomeCopy, r: HomeProbeResult): { readonly text: string; readonly problem: boolean } {
  const c = copy.result;
  switch (r.kind) {
    case "invalid_url": return { text: copy.invalid[r.reason], problem: true };
    case "ok": return { text: `${c.ok(r.providerId)} ${c.health[r.health]} ${copy.selfReported}`, problem: r.health !== "healthy" };
    case "unreachable": return { text: c.unreachable, problem: true };
    case "failing": return { text: c.failing(r.status), problem: true };
    case "not_a_home": return { text: c.notAHome, problem: true };
    case "wrong_version": return { text: c.wrongVersion(r.offered.join(", ")), problem: true };
    case "no_mailbox": return { text: c.noMailbox, problem: true };
  }
}
