// SPDX-License-Identifier: MPL-2.0
// local_delivery slice 4 feasibility spike (ADR-0158 section 4, gate V-024): can each browser engine establish an ordered,
// reliable RTCDataChannel labelled `om-lan-v1` between two peers using host candidates only (no STUN, no TURN, no signalling
// service; the offer and answer are exchanged by this script the way copy/paste or QR would), and stream 16 KiB frames with
// the profile's backpressure marks? It records, per engine: established, candidate types seen (host / mDNS / srflx),
// frames and bytes received in order, and whether bufferedAmountLowThreshold events fire.
// Run: node feasibility.mjs [--json out.json]   (needs the `playwright` package and its browsers; run on the build host)
import { chromium, firefox, webkit } from "playwright";
import { writeFileSync } from "node:fs";

const FRAME = 16384, FRAMES = 256, HIGH = 262144, LOW = 65536;

const PEER = `(() => {
  window.__om = {
    async make(role) {
      const pc = new RTCPeerConnection({ iceServers: [] });
      const st = { pc, got: 0, bytes: 0, order: true, last: -1, lowEvents: 0, maxBuffered: 0, open: false, cands: [] };
      window.__st = st;
      pc.onicecandidate = (e) => { if (e.candidate) st.cands.push(e.candidate.candidate); };
      const wire = (ch) => {
        st.ch = ch; ch.binaryType = "arraybuffer";
        ch.onopen = () => { st.open = true; };
        ch.onmessage = (m) => { const v = new Uint8Array(m.data); const n = (v[0] << 8) | v[1]; if (n !== st.last + 1) st.order = false; st.last = n; st.got++; st.bytes += v.byteLength; };
        ch.bufferedAmountLowThreshold = ${LOW};
        ch.onbufferedamountlow = () => { st.lowEvents++; };
      };
      if (role === "offer") wire(pc.createDataChannel("om-lan-v1", { ordered: true }));
      else pc.ondatachannel = (e) => wire(e.channel);
      return true;
    },
    async gathered() { const pc = window.__st.pc; if (pc.iceGatheringState === "complete") return; await new Promise((r) => { pc.addEventListener("icegatheringstatechange", () => pc.iceGatheringState === "complete" && r()); setTimeout(r, 8000); }); },
    async offer() { const pc = window.__st.pc; await pc.setLocalDescription(await pc.createOffer()); await this.gathered(); return pc.localDescription.sdp; },
    async answer(sdp) { const pc = window.__st.pc; await pc.setRemoteDescription({ type: "offer", sdp }); await pc.setLocalDescription(await pc.createAnswer()); await this.gathered(); return pc.localDescription.sdp; },
    async accept(sdp) { await window.__st.pc.setRemoteDescription({ type: "answer", sdp }); },
    async opened() { const s = window.__st; for (let i = 0; i < 100 && !s.open; i++) await new Promise((r) => setTimeout(r, 100)); return s.open; },
    async stream() {
      const s = window.__st, ch = s.ch;
      for (let i = 0; i < ${FRAMES}; i++) {
        if (ch.bufferedAmount > ${HIGH}) await new Promise((r) => { const f = () => { ch.removeEventListener("bufferedamountlow", f); r(); }; ch.addEventListener("bufferedamountlow", f); });
        const b = new Uint8Array(${FRAME}); b[0] = i >> 8; b[1] = i & 255; ch.send(b);
        s.maxBuffered = Math.max(s.maxBuffered, ch.bufferedAmount);
      }
      return { maxBuffered: s.maxBuffered, lowEvents: s.lowEvents };
    },
    async report() { const s = window.__st; for (let i = 0; i < 100 && s.got < ${FRAMES}; i++) await new Promise((r) => setTimeout(r, 100)); return { got: s.got, bytes: s.bytes, inOrder: s.order, cands: s.cands, ordered: s.ch.ordered, label: s.ch.label, reliable: s.ch.maxRetransmits === null && s.ch.maxPacketLifeTime === null }; },
  };
})()`;

const out = {};
for (const [name, type] of [["chromium", chromium], ["firefox", firefox], ["webkit", webkit]]) {
  const r = { engine: name };
  let b;
  try {
    b = await type.launch();
    const ctx = await b.newContext();
    const [a, c] = [await ctx.newPage(), await ctx.newPage()];
    for (const p of [a, c]) { await p.goto("about:blank"); await p.evaluate(PEER); }
    await a.evaluate(() => window.__om.make("offer")); await c.evaluate(() => window.__om.make("answer"));
    const offer = await a.evaluate(() => window.__om.offer());
    const answer = await c.evaluate((s) => window.__om.answer(s), offer);
    await a.evaluate((s) => window.__om.accept(s), answer);
    r.established = (await a.evaluate(() => window.__om.opened())) && (await c.evaluate(() => window.__om.opened()));
    if (r.established) {
      const sent = await a.evaluate(() => window.__om.stream());
      const got = await c.evaluate(() => window.__om.report());
      const kinds = (l) => [...new Set(l.map((x) => (/ typ (\w+)/.exec(x) || [])[1] + (/\.local /.test(x) ? "+mdns" : "")))];
      Object.assign(r, { frames: got.got, bytes: got.bytes, inOrder: got.inOrder, ordered: got.ordered, reliable: got.reliable, label: got.label, sentMaxBuffered: sent.maxBuffered, lowEvents: sent.lowEvents, offerCandidateTypes: kinds(offer.split("\n").filter((l) => l.startsWith("a=candidate")).map((l) => l.slice(2))), sdpHasStun: /stun:|typ srflx|typ relay/.test(offer + answer) });
      r.pass = got.got === FRAMES && got.inOrder && got.ordered && got.reliable;
    }
  } catch (e) { r.error = String(e).slice(0, 300); }
  finally { await b?.close(); }
  out[name] = r;
  console.log(JSON.stringify(r));
}
const i = process.argv.indexOf("--json");
if (i > 0) writeFileSync(process.argv[i + 1], JSON.stringify({ kind: "lan_datachannel_feasibility", frame: FRAME, frames: FRAMES, high: HIGH, low: LOW, engines: out }, null, 1) + "\n");
