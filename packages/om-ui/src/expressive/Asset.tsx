// SPDX-License-Identifier: MPL-2.0
import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { DecodedExpressive, ExpressiveFrame, ExpressiveSource, MessageId } from "@om/platform-api";
import type { ConversationServices } from "../conversations/services";
import { Button } from "../shell/primitives/Button";
import { useReducedMotion } from "../shell/theme/useReducedMotion";
import type { ExpressiveDescription, ExpressiveSurface } from "./host";
import "./expressive.css";

type Decoded = Extract<DecodedExpressive, { state: "decoded" }>;

interface AssetProps {
  readonly services: ConversationServices;
  readonly surface: ExpressiveSurface;
  readonly messageId: MessageId;
  readonly source: ExpressiveSource;
  readonly size: "message" | "inline";
  readonly fallback?: ReactNode;
  readonly revision?: string | undefined;
  readonly interactive?: boolean;
}

export function ExpressiveAsset({ services, surface, messageId, source, size, fallback, revision, interactive = true }: AssetProps) {
  const { ctx, client } = services;
  const { describe } = surface;
  const [description, setDescription] = useState<ExpressiveDescription | null>(null);
  const kind = source.kind;
  const index = source.kind === "reaction" ? -1 : source.index;
  const key = source.kind === "reaction" ? source.key : "";
  const mine = source.kind === "reaction" && source.mine;
  useEffect(() => {
    let current = true;
    const target: ExpressiveSource = kind === "reaction" ? { kind, key, mine } : { kind, index };
    setDescription(null);
    void describe(ctx, messageId, target).then((value) => {
      if (!current) return;
      setDescription(value);
    }, () => { /* the ordinary fallback remains visible */ });
    return () => { current = false; };
  }, [ctx, describe, messageId, kind, index, key, mine, revision]);
  if (description === null) return fallback ?? null;
  return <AssetFrame api={client.expressive} {...{ ctx, surface, messageId, size, description, interactive }}
    source={kind === "reaction" ? { kind, key, mine } : { kind, index }} />;
}

function AssetFrame({ api, ctx, surface, messageId, source, size, description, interactive }: {
  readonly api: ConversationServices["client"]["expressive"];
  readonly ctx: ConversationServices["ctx"];
  readonly surface: ExpressiveSurface;
  readonly messageId: MessageId;
  readonly source: ExpressiveSource;
  readonly size: AssetProps["size"];
  readonly description: ExpressiveDescription;
  readonly interactive: boolean;
}) {
  const { decoder, copy } = surface;
  const calm = useReducedMotion();
  const region = useRef<HTMLSpanElement>(null);
  const [requested, setRequested] = useState(description.local === true ? 1 : 0);
  const [result, setResult] = useState<DecodedExpressive | null>(null);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  // Inline media can occur in a link label. Only open local static frames there.
  const openable = description.canOpen && decoder !== undefined && api !== undefined && (interactive || description.local === true);
  const kind = source.kind;
  const index = source.kind === "reaction" ? -1 : source.index;
  const key = source.kind === "reaction" ? source.key : "";
  const mine = source.kind === "reaction" && source.mine;
  useEffect(() => {
    if (!openable || requested === 0) return;
    let current = true;
    let accepted: DecodedExpressive | null = null;
    setLoading(true);
    setFailed(false);
    setResult(null);
    const target: ExpressiveSource = kind === "reaction" ? { kind, key, mine } : { kind, index };
    void api.decode(ctx, messageId, target, decoder, { reducedMotion: calm || !interactive }).then((value) => {
      if (!current) { if (value?.state === "decoded") value.playback?.close(); return; }
      accepted = value;
      setResult(value);
      setFailed(value?.state !== "decoded");
      setLoading(false);
    }, () => {
      if (current) { setFailed(true); setLoading(false); }
    });
    return () => { current = false; if (accepted?.state === "decoded") accepted.playback?.close(); };
  }, [api, ctx, decoder, messageId, requested, openable, calm, interactive, kind, index, key, mine]);

  const controls = interactive;
  const open = () => {
    region.current?.focus({ preventScroll: true });
    setRequested((n) => n + 1);
  };
  return <span ref={region} tabIndex={controls ? -1 : undefined} className="om-expressive-asset" data-size={size}>
    {result?.state === "decoded" ? <Canvas key={requested} decoded={result} manual={calm || !controls} controls={controls} copy={copy}
      onFailure={() => { setResult(null); setFailed(true); }} /> : <bdi>{result?.altText ?? description.altText}</bdi>}
    {controls && loading ? <span role="status">{copy.loading}</span> : null}
    {controls && (failed || !description.canOpen || decoder === undefined) ? <span role="status">{copy.unavailable}</span> : null}
    {controls && openable && !loading && (failed || requested === 0) ? <Button variant="quiet" onClick={open}>{failed ? copy.retry : copy.show}</Button> : null}
  </span>;
}

function Canvas({ decoded, manual, controls, copy, onFailure }: {
  readonly decoded: Decoded;
  readonly manual: boolean;
  readonly controls: boolean;
  readonly copy: ExpressiveSurface["copy"];
  readonly onFailure: () => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const { width, height } = decoded.facts;
  const playback = decoded.playback;
  const [playing, setPlaying] = useState(!manual && playback !== undefined);
  const duration = useRef<number | null>(null);
  const failure = useRef(onFailure);
  failure.current = onFailure;

  useEffect(() => {
    const context = canvas.current?.getContext("2d");
    if (context == null) { playback?.close(); failure.current(); return; }
    // The driver supplies the dimensions and the RGBA buffer. Never create an image URL or use a pack's layout.
    const draw = (frame: ExpressiveFrame) => {
      context.putImageData(new ImageData(frame.rgba, width, height), 0, 0);
      duration.current = frame.durationUs;
    };
    try { draw(decoded.frame); } catch { playback?.close(); failure.current(); }
  }, [decoded, playback, width, height]);

  useEffect(() => {
    if (!playing || playback === undefined) return;
    const context = canvas.current?.getContext("2d");
    if (context == null) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    const failed = () => { if (current) { playback.close(); failure.current(); } };
    const next = () => {
      timer = setTimeout(() => {
        void playback.nextFrame().then((result) => {
          if (!current) return;
          if (result.state !== "frame") { failed(); return; }
          context.putImageData(new ImageData(result.frame.rgba, width, height), 0, 0);
          duration.current = result.frame.durationUs;
          next();
        }).catch(failed);
      }, Math.max(16, (duration.current ?? 33_000) / 1000));
    };
    playback.play();
    next();
    return () => { current = false; clearTimeout(timer); playback.pause(); };
  }, [playing, playback, width, height]);

  return <>
    <canvas ref={canvas} width={width} height={height} role="img" aria-label={decoded.altText} />
    {playback === undefined || !controls ? null : <Button variant="quiet" onClick={() => { setPlaying((p) => !p); }}>{playing ? copy.pause : copy.play}</Button>}
  </>;
}
