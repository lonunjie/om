import { useEffect, useRef, useState } from "react";
import { Button, CallView } from "@om/ui";
import type { CallView as CoreCallView } from "@om/platform-api";
import { callCopy as t } from "../copy/en";
import type { useCalls } from "./useCalls";

type Calls = NonNullable<ReturnType<typeof useCalls>>;

interface DeviceChoiceProps {
  readonly label: string;
  readonly devices: readonly MediaDeviceInfo[];
  readonly choose: (id: string) => void;
}
function DeviceChoice({ label, devices, choose }: DeviceChoiceProps) {
  if (devices.length === 0) return null;
  return (
    <label className="om-call__device">
      <span>{label}</span>
      <select defaultValue="" onChange={(e) => { choose(e.currentTarget.value); }}>
        <option value="" disabled>{t.defaultDevice}</option>
        {devices.map((device, i) => (
          <option key={device.deviceId} value={device.deviceId}>{device.label || t.deviceChoice(label, i + 1)}</option>
        ))}
      </select>
    </label>
  );
}

function LiveMedia({ call, calls }: { readonly call: CoreCallView; readonly calls: Calls }) {
  const element = useRef<HTMLVideoElement>(null);
  const [needsPlay, setNeedsPlay] = useState(false);
  const [outputFailed, setOutputFailed] = useState(false);
  const remote = calls.remote;
  useEffect(() => {
    const player = element.current;
    if (remote === null || player === null) return;
    let live = true;
    player.srcObject = remote;
    player.play().then(() => { if (live) setNeedsPlay(false); }, () => { if (live) setNeedsPlay(true); });
    return () => { live = false; player.srcObject = null; };
  }, [remote]);

  const hasSink = typeof HTMLMediaElement !== "undefined" && "setSinkId" in HTMLMediaElement.prototype;
  const video = call.media === "video" && (call.peerMediaFlags & 2) !== 0;
  return (
    <>
      <video ref={element} playsInline className="om-call__media" hidden={!video || remote === null} />
      {needsPlay && remote !== null ? <Button variant="quiet" onClick={() => {
        void element.current?.play().then(() => { setNeedsPlay(false); }, () => { setNeedsPlay(true); });
      }}>{t.playAudio}</Button> : null}
      {call.state === "active" || call.state === "reconnecting" ? <div className="om-call__controls">
        <DeviceChoice label={t.microphone} devices={calls.devices.filter((d) => d.kind === "audioinput" && d.deviceId !== "")} choose={(id) => { calls.selectInput("audio", id); }} />
        {call.media === "video" && calls.cameraAvailable
          ? <DeviceChoice label={t.camera} devices={calls.devices.filter((d) => d.kind === "videoinput" && d.deviceId !== "")} choose={(id) => { calls.selectInput("video", id); }} /> : null}
        {!hasSink ? null : <DeviceChoice label={t.speaker} devices={calls.devices.filter((d) => d.kind === "audiooutput" && d.deviceId !== "")} choose={(id) => {
          void element.current?.setSinkId(id).then(() => { setOutputFailed(false); }, () => { setOutputFailed(true); });
        }} />}
        {outputFailed ? <p role="alert">{t.outputFailed}</p> : null}
      </div> : null}
    </>
  );
}

export function CallHost({ calls, name, identity }: { readonly calls: Calls; readonly name: string; readonly identity: string | undefined }) {
  const call = calls.view;
  const live = call !== null && ["connecting", "active", "reconnecting"].includes(call.state);
  return <CallView
    copy={t} call={call} name={name} identity={identity} pending={calls.pending}
    privacyAllowsDirect={calls.privacyAllowsDirect} cameraAvailable={calls.cameraAvailable}
    error={calls.failure === null ? null : t.errors[calls.failure]}
    media={live ? <LiveMedia key={call.callSessionId} call={call} calls={calls} /> : undefined}
    onAnswer={calls.answer} onEnd={calls.end} onToggle={calls.toggle} onDismiss={calls.dismiss}
  />;
}
