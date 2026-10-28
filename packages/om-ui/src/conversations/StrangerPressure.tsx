import { useEffect, useState } from "react";
import type { IntroductionLoad } from "@om/platform-api";
import { Text } from "../shell/primitives/Text";
import type { ConversationCopy } from "./copy";
import type { ConversationServices } from "./services";

/**
 * One aggregated notice while requests from strangers are being dropped (ADR-0181 section 5): never a silent empty
 * inbox, never one notice per request. People the user invited can still reach them.
 */
export function StrangerPressure({ services, copy }: { readonly services: ConversationServices; readonly copy: ConversationCopy["pressure"] }) {
  const [load, setLoad] = useState<IntroductionLoad | null>(null);
  useEffect(() => {
    let live = true;
    let unsubscribe: (() => void) | undefined;
    try { unsubscribe = services.client.subscribe({ kind: "GetIntroductionLoad", ctx: services.ctx }, (r) => { if (live) setLoad(r); }); } catch { /* stale context */ }
    services.client.query({ kind: "GetIntroductionLoad", ctx: services.ctx }).then((r) => { if (live) setLoad(r); }, () => { if (live) setLoad(null); });
    return () => { live = false; unsubscribe?.(); };
  }, [services.client, services.ctx]);
  if (load === null || !load.throttled) return null;
  return <Text as="p" role="status" className="om-pressure" data-testid="stranger-pressure">{copy.notice}</Text>;
}
