// SPDX-License-Identifier: MPL-2.0
import { useEffect, useRef, useState } from "react";
import type { ConversationId, ExpressiveApi, ExpressivePackPreview, ExpressivePicker, ExpressiveQueryResult, MessageId } from "@om/platform-api";
import { resolveIntent, settledOutcome, type ConversationServices } from "../conversations/services";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import type { ExpressiveCopy } from "./copy";
import type { ExpressiveSurface } from "./host";
import "./expressive.css";

type PickerItem = ExpressivePicker["items"][number];
type Packs = ExpressiveQueryResult["packs"];
type Panel =
  | { kind: "packs"; packs: Packs }
  | { kind: "items" | "details" | "remove"; picker: ExpressivePicker }
  | { kind: "review"; preview: ExpressivePackPreview; updating: boolean };

interface ControlProps {
  readonly services: ConversationServices;
  readonly surface: ExpressiveSurface;
  readonly conversationId: ConversationId;
  readonly reactionTo?: MessageId;
}

export function ExpressivePickerControl(props: ControlProps) {
  const [open, setOpen] = useState(false);
  if (props.services.client.expressive?.available() !== true) return null;
  return <>
    <Button variant="quiet" aria-haspopup="dialog" onClick={() => { setOpen(true); }}>{props.surface.copy.picker}</Button>
    {open ? <ExpressivePickerSheet {...props} onClose={() => { setOpen(false); }} /> : null}
  </>;
}

export function ExpressivePickerSheet({ onClose, ...props }: ControlProps & { readonly onClose: () => void }) {
  const api = props.services.client.expressive;
  if (api?.available() !== true) return null;
  const title = props.reactionTo === undefined ? props.surface.copy.picker : props.surface.copy.reactions;
  return <Sheet title={title} closeLabel={props.surface.copy.close} onClose={onClose}>
    <Picker {...props} api={api} />
  </Sheet>;
}

function Provenance({ signedBy, copy }: { readonly signedBy: readonly (readonly number[])[]; readonly copy: ExpressiveCopy }) {
  if (signedBy.length === 0) return <p>{copy.unsigned}</p>;
  return <div>{copy.signedBy}
    <ul>{signedBy.map((id) => {
      const evidence = id.map((byte) => byte.toString(16).padStart(2, "0")).join("");
      return <li key={evidence}><bdi>{evidence}</bdi></li>;
    })}</ul>
  </div>;
}

function PackReview({ preview, copy }: { readonly preview: ExpressivePackPreview; readonly copy: ExpressiveCopy }) {
  // Every author-supplied field is inert, direction-isolated text. Source addresses never become links.
  return <>
    <h3 tabIndex={-1} data-expressive-heading><bdi>{preview.title}</bdi></h3>
    {preview.description === null ? null : <p><bdi>{preview.description}</bdi></p>}
    {preview.creatorDisplay === null ? null : <p>{copy.creator} <bdi>{preview.creatorDisplay}</bdi></p>}
    <Provenance signedBy={preview.signedBy} copy={copy} />
    <p>{copy.release(preview.sequence)}</p>
    <p>{copy.controllers(preview.threshold, preview.controllerCount)}</p>
    <p>{copy.itemCount(preview.itemCount)} · {copy.size(preview.size)}</p>
    {preview.spdxId === null && preview.licenseUrl === null && preview.licenseTextHash === null ? <p>{copy.undeclaredLicense}</p> : (
      <p>{copy.declaredLicense} <bdi>{[preview.spdxId, preview.licenseUrl, preview.licenseTextHash].filter(Boolean).join(" · ")}</bdi></p>
    )}
    <p>{copy.perDevice}</p>
  </>;
}

// A profile switch or dismissal can abort while a Core promise is pending.
const cancelled = (signal: AbortSignal): boolean => signal.aborted;

function Picker({ services, surface, conversationId, reactionTo, api }: ControlProps & { readonly api: ExpressiveApi }) {
  const { copy } = surface;
  const { ctx } = services;
  const [panel, setPanel] = useState<Panel | null>(null);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const life = useRef<AbortController | null>(null);
  const locked = useRef(false);
  const targetTitle = panel?.kind;
  const readPacks = () => api.query(ctx, { kind: "packs" });

  useEffect(() => {
    const job = new AbortController();
    life.current = job;
    void api.query(ctx, { kind: "packs" }).then(
      (packs) => { if (!job.signal.aborted) setPanel({ kind: "packs", packs }); },
      () => { if (!job.signal.aborted) setStatus(copy.loadFailed); },
    );
    return () => { job.abort(); };
  }, [api, ctx, copy.loadFailed]);

  useEffect(() => {
    root.current?.querySelector<HTMLElement>("[data-expressive-heading]")?.focus({ preventScroll: true });
  }, [targetTitle]);

  const run = (working: string, failure: string, work: (signal: AbortSignal) => Promise<void>) => {
    const signal = life.current?.signal;
    if (locked.current || signal === undefined || cancelled(signal)) return;
    locked.current = true;
    setBusy(true);
    setStatus(working);
    void work(signal).catch(() => {
      if (!cancelled(signal)) setStatus(failure);
    }).finally(() => {
      locked.current = false;
      if (!cancelled(signal)) setBusy(false);
    });
  };

  const openPack = (packId: string) => {
    run(copy.loading, copy.loadFailed, async (signal) => {
      const picker = await api.query(ctx, { kind: "picker", packId });
      if (cancelled(signal)) return;
      if (picker === null) { setStatus(copy.loadFailed); return; }
      setPanel({ kind: "items", picker });
      setStatus("");
    });
  };

  const back = () => {
    run(copy.loading, copy.loadFailed, async (signal) => {
      const packs = await readPacks();
      if (cancelled(signal)) return;
      setPanel({ kind: "packs", packs });
      setStatus("");
    });
  };

  const mutate = async (actionId: string, params: Readonly<Record<string, unknown>>) => {
    const r = services.registry.resolve({ actionId, params, ctx, origin: "direct" });
    return r.status === "expressive" && "accepted" in await api.do(ctx, r.intent);
  };

  const selection = (picker: ExpressivePicker, item: PickerItem) => ({
    packId: picker.packId, releaseId: picker.pack.releaseId, itemId: item.itemId,
  });
  const reactKey = (item: PickerItem): string | null => {
    // The frozen carriage leaves the token free. Only use the pack's explicit, valid fallback.
    const key = item.fallbackText;
    return key !== null && key.trim() !== "" && !/\p{Cc}/u.test(key) && new TextEncoder().encode(key).length <= 64 ? key : null;
  };

  const useItem = (picker: ExpressivePicker, item: PickerItem) => {
    const failure = reactionTo === undefined ? copy.sendFailed : copy.reactionFailed;
    run(reactionTo === undefined ? copy.sending : copy.reacting, failure, async (signal) => {
      const key = reactKey(item);
      if (pending || (reactionTo !== undefined && key === null)) return;
      const selected = selection(picker, item);
      const bytes = await surface.readCached?.(ctx, item.blobId);
      if (cancelled(signal)) return;
      if (bytes == null) { setStatus(copy.missingItem); return; }
      const prepared = await api.prepare(ctx, selected, bytes);
      if (cancelled(signal)) return;
      if (!prepared.ok) { setStatus(failure); return; }
      const actionId = reactionTo === undefined ? "send-sticker" : "set-custom-reaction";
      const r = resolveIntent(services, actionId, {
        conversationId, blobId: prepared.blobId, ...(reactionTo === undefined ? {} : { messageId: reactionTo, key }),
      });
      if (r.status !== "intent") { setStatus(failure); return; }
      // A lost acknowledgement can still be a send. Keep it pending and never create a new operation as a retry.
      setPending(true);
      setStatus(copy.pending);
      const handle = await services.client.dispatch(r.intent).catch(() => null);
      if (handle === null) return;
      const outcome = await settledOutcome(services, handle.operationId).catch(() => null);
      if (!cancelled(signal)) {
        if (outcome?.kind === "Rejected") { setPending(false); setStatus(failure); }
        if (outcome?.kind === "NeedsUserAction") setStatus(copy.needsAction[outcome.action]);
        if (outcome?.kind === "PendingAuthorization") setStatus(copy.approval);
        if (outcome?.kind === "Committed") { setPending(false); setStatus(reactionTo === undefined ? copy.sent : copy.reacted); }
      }
      if (outcome?.kind !== "Committed") return;
      try {
        // Closing the sheet after sending does not undo the committed selection.
        const recent = await api.do(ctx, { ...selected, kind: "update_picker", change: { kind: "recent" } });
        if (cancelled(signal)) return;
        const next = await api.query(ctx, { kind: "picker", packId: picker.packId });
        if (cancelled(signal)) return;
        if (next !== null) setPanel({ kind: "items", picker: next });
        if (!("accepted" in recent) || next === null) setStatus(copy.recentFailed);
      } catch {
        if (!cancelled(signal)) setStatus(copy.recentFailed);
      }
    });
  };

  const showItems = (picker: ExpressivePicker) => {
    const eligible = picker.items.filter((item) => reactionTo === undefined
      ? item.kind === "sticker" || item.kind === "loop_animation"
      : (item.kind === "reaction_asset" || item.kind === "custom_emoji") && reactKey(item) !== null);
    const byId = new Map(eligible.map((item) => [item.itemId, item]));
    const recent = picker.recents.flatMap((id) => byId.get(id) ?? []);
    const groups = [
      { heading: copy.recents, items: recent },
      { heading: copy.favourites, items: eligible.filter((item) => item.favourite) },
      { heading: copy.items, items: eligible },
    ];
    return <>
      <Button variant="quiet" disabled={busy} onClick={back}>{copy.back}</Button>
      <h3 tabIndex={-1} data-expressive-heading><bdi>{picker.pack.title}</bdi></h3>
      <Button variant="quiet" disabled={busy} onClick={() => { setStatus(""); setPanel({ kind: "details", picker }); }}>{copy.details(picker.pack.title)}</Button>
      {eligible.length === 0 ? <p>{copy.emptyItems}</p> : null}
      {groups.filter((group) => group.items.length !== 0).map(({ heading, items }) => (
        <section key={heading} aria-label={heading}>
          <h4>{heading}</h4>
          <ul className="om-expressive-picker__items">{items.map((item) => {
            const available = item.cached && surface.readCached !== undefined;
            return <li key={item.itemId}>
              <Button disabled={!available || busy || pending} onClick={() => { useItem(picker, item); }}
                aria-label={(reactionTo === undefined ? copy.send : copy.react)(item.label)}
                title={item.altText}>
                <bdi>{(reactionTo === undefined ? copy.send : copy.react)(item.label)}</bdi>
              </Button>
              <Button variant="quiet" disabled={busy} onClick={() => {
                run(copy.loading, copy.favouriteFailed, async (signal) => {
                  if (!await mutate("set-expressive-favourite", { ...selection(picker, item), present: !item.favourite })) throw new Error("favourite refused");
                  if (cancelled(signal)) return;
                  const next = await api.query(ctx, { kind: "picker", packId: picker.packId });
                  if (cancelled(signal)) return;
                  if (next === null) throw new Error("pack missing");
                  setPanel({ kind: "items", picker: next });
                  setStatus("");
                });
              }}><bdi>{(item.favourite ? copy.removeFavourite : copy.addFavourite)(item.label)}</bdi></Button>
              {available ? null : <span>{copy.missingItem}</span>}
            </li>;
          })}</ul>
        </section>
      ))}
    </>;
  };

  const review = surface.review;
  return (
    <div ref={root} className="om-expressive-picker">
      <p role="status">{panel === null && status === "" ? copy.loading : status}</p>
      {panel?.kind === "packs" ? <>
        <h3 data-expressive-heading tabIndex={-1}>{copy.packs}</h3>
        {panel.packs.length === 0 ? <p>{copy.empty}</p> : <ul className="om-expressive-picker__items">{panel.packs.map(([id, pack]) => (
          <li key={id}><Button disabled={busy} onClick={() => { openPack(id); }}><bdi>{pack.title}</bdi></Button></li>
        ))}</ul>}
        {review === undefined ? null : <Button disabled={busy} onClick={() => {
          run(copy.loading, copy.reviewFailed, async (signal) => {
            const result = await api.query(ctx, { kind: "preview_pack", input: review.input });
            if (cancelled(signal)) return;
            if ("refused" in result) { setStatus(copy.reviewFailed); return; }
            const current = panel.packs.find(([id]) => id === result.preview.packId)?.[1];
            setPanel({ kind: "review", preview: result.preview, updating: current !== undefined && current.releaseId !== result.preview.releaseId });
            setStatus("");
          });
        }}>{copy.review}</Button>}
      </> : null}
      {panel?.kind === "items" ? showItems(panel.picker) : null}
      {panel?.kind === "details" || panel?.kind === "remove" ? <>
        <Button variant="quiet" disabled={busy} onClick={() => { setPanel({ kind: "items", picker: panel.picker }); setStatus(""); }}>{copy.back}</Button>
        <h3 data-expressive-heading tabIndex={-1}><bdi>{panel.kind === "remove" ? copy.removeQuestion(panel.picker.pack.title) : panel.picker.pack.title}</bdi></h3>
        {panel.kind === "details" ? <>
          <Provenance signedBy={panel.picker.pack.signedBy} copy={copy} />
          <p>{copy.itemCount(panel.picker.pack.itemCount)}</p>
          <p>{copy.perDevice}</p>
          <Button variant="quiet" onClick={() => { setPanel({ kind: "remove", picker: panel.picker }); }}>{copy.remove(panel.picker.pack.title)}</Button>
        </> : <>
          <p>{copy.removeEffect}</p>
          <Button variant="danger" disabled={busy} onClick={() => {
            run(copy.removing, copy.removeFailed, async (signal) => {
              if (!await mutate("remove-expressive-pack", { packId: panel.picker.packId })) throw new Error("removal refused");
              if (cancelled(signal)) return;
              // The mutation has committed even if refreshing the list fails.
              setPanel(null);
              setStatus(copy.removed);
              const packs = await readPacks().catch(() => null);
              if (cancelled(signal)) return;
              if (packs !== null) setPanel({ kind: "packs", packs });
              else setStatus(`${copy.removed} ${copy.loadFailed}`);
            });
          }}><bdi>{copy.remove(panel.picker.pack.title)}</bdi></Button>
        </>}
      </> : null}
      {panel?.kind === "review" && review !== undefined ? <>
        <Button variant="quiet" disabled={busy} onClick={back}>{copy.back}</Button>
        <PackReview preview={panel.preview} copy={copy} />
        <Button disabled={busy} variant="primary" onClick={() => {
          run(copy.installing, copy.installFailed, async (signal) => {
            if (!await mutate("install-expressive-pack", {
              input: review.input, releaseId: panel.preview.releaseId, controlDigest: panel.preview.controlDigest, cached: review.cached ?? [],
            })) throw new Error("install refused");
            if (cancelled(signal)) return;
            setPanel(null);
            setStatus(copy.installed);
            const packs = await readPacks().catch(() => null);
            if (cancelled(signal)) return;
            if (packs !== null) setPanel({ kind: "packs", packs });
            else setStatus(`${copy.installed} ${copy.loadFailed}`);
          });
        }}><bdi>{(panel.updating ? copy.update : copy.install)(panel.preview.title)}</bdi></Button>
      </> : null}
      {panel === null && status !== "" && !busy ? <Button onClick={back}>{copy.retry}</Button> : null}
    </div>
  );
}
