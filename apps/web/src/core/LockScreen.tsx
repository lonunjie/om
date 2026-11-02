// Settings > Lock: how this device opens OM, said plainly. Read-only until the vault can change its lock (no flow that ends in
// "unsupported" is offered): a key kept in this browser is called out as "not locked" because that is what a person needs to know.
import { useEffect, useState } from "react";
import type { VaultWrappingInfo } from "@om/platform-api";
import { Icon, Text } from "@om/ui";
import { lockCopy as c } from "../copy/en";

export interface LockServices {
  /** How the active identity's vault is wrapped on this device; null when it cannot be read. */
  readonly wrapping: () => Promise<VaultWrappingInfo | null>;
}

type Mode = "none" | "device" | "device-shared" | "password";
const modeOf = (w: VaultWrappingInfo): Mode =>
  w.passphraseProtected ? "password" : w.method === "device-key" ? "none" : w.reducedAtRestProperty ? "device-shared" : "device";

export function LockScreen({ services }: { readonly services: LockServices }) {
  const [state, setState] = useState<{ readonly k: "loading" } | { readonly k: "failed" } | { readonly k: "ready"; readonly mode: Mode }>({ k: "loading" });
  useEffect(() => {
    let live = true;
    services.wrapping().then((w) => { if (live) setState(w === null ? { k: "failed" } : { k: "ready", mode: modeOf(w) }); }, () => { if (live) setState({ k: "failed" }); });
    return () => { live = false; };
  }, [services]);
  return (
    <section className="om-lock" aria-labelledby="om-lock-h">
      <Text as="h1" size="xl" weight="bold" id="om-lock-h">{c.heading}</Text>
      {state.k === "loading" ? <Text tone="secondary" role="status">{c.loading}</Text> : null}
      {state.k === "failed" ? <Text role="alert">{c.failed}</Text> : null}
      {state.k === "ready" ? (
        <div className="om-lock__state" data-mode={state.mode}>
          <span className="om-lock__icon" aria-hidden="true"><Icon name="lock" size={22} /></span>
          <div className="om-lock__text">
            <Text as="h2" size="md" weight="bold">{c.modes[state.mode].title}</Text>
            <Text as="p" size="sm">{c.modes[state.mode].body}</Text>
            {state.mode === "none" ? <Text as="p" size="sm" weight="bold" role="note">{c.warning}</Text> : null}
          </div>
        </div>
      ) : null}
      <Text as="p" size="sm" tone="secondary">{c.fixed}</Text>
    </section>
  );
}
