#!/usr/bin/env node
// USB helper for the OM root-key ceremony (deploy/KEY-CEREMONY.md). Talks to a Ledger Nano S+ running the Solana app.
//   node ledger-sign.mjs check
//   node ledger-sign.mjs pubkey [--path "44'/501'/0'"] [--display]
//   node ledger-sign.mjs sign --payload <file from root-ceremony.mjs external-payload> [--path "44'/501'/0'"] [--out <file>]
// It sends nothing but the envelope you give it and prints only public data (public key, signature). Run it on the
// machine the Ledger is plugged into, with the Solana app open on the device. Isolated package: not in the repo workspace.
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { base58, checkPath, parseEnvelope, signEnvelope } from "./lib.mjs";

const require = createRequire(import.meta.url);
const args = process.argv.slice(2);
const cmd = args.shift();
const opt = (k, d) => { const i = args.indexOf(`--${k}`); return i >= 0 ? args[i + 1] : d; };
const fail = (m) => { console.error(`ledger-sign: ${m}`); process.exit(1); };
const MIN_APP = [1, 3, 0]; // off-chain signing was added in Solana app 1.3.0 (app-solana doc/api.md)

const open = async () => {
  let Transport, Solana;
  try {
    Transport = require("@ledgerhq/hw-transport-node-hid").default;
    Solana = require("@ledgerhq/hw-app-solana").default;
  } catch (e) { fail(`cannot load the Ledger packages (${e.code ?? e.message}). In deploy/ledger run: pnpm install --frozen-lockfile, then approve and rebuild node-hid (deploy/KEY-CEREMONY.md, "Ledger helper").`); }
  let transport;
  try { transport = await Transport.create(); } catch (e) {
    if (/bindings file|HID\.node/.test(String(e.message))) fail("the native node-hid module is not built. Approve and build it first (deploy/KEY-CEREMONY.md, \"Ledger helper\": pnpm approve-builds, then pnpm rebuild node-hid).");
    fail(`no Ledger found over USB: ${e.message}. Plug it in, unlock it and open the Solana app.`);
  }
  return { transport, solana: new Solana(transport) };
};
const explain = (e) => {
  const sw = e?.statusCode;
  const hints = { 0x6985: "rejected on the device", 0x6982: "rejected on the device", 0x6a81: "the app rejected the message header", 0x6a82: "the app rejected the message format", 0x6808: "blind signing is needed for this message (it should not be, check the payload)", 0x6e00: "wrong app open: open the Solana app", 0x6d00: "wrong app open: open the Solana app", 0x5515: "device is locked: unlock it" };
  return `${e?.message ?? e}${sw !== undefined ? ` [status 0x${sw.toString(16)}${hints[sw] ? `: ${hints[sw]}` : ""}]` : ""}`;
};

const run = async () => {
  if (cmd === "check") {
    const { transport, solana } = await open();
    try {
      const c = await solana.getAppConfiguration();
      const v = c.version.split(".").map(Number);
      const old = v[0] < MIN_APP[0] || (v[0] === MIN_APP[0] && (v[1] < MIN_APP[1]));
      console.log(JSON.stringify({ solanaAppVersion: c.version, blindSigningEnabled: c.blindSigningEnabled, ok: !old }));
      if (old) fail(`Solana app ${c.version} is older than ${MIN_APP.join(".")}; update it in Ledger Live`);
      console.error("Blind signing may stay OFF: the ceremony uses restricted-ASCII messages, which the app shows in clear.");
    } finally { await transport.close(); }
  } else if (cmd === "pubkey") {
    const path = checkPath(opt("path", "44'/501'/0'"));
    const { transport, solana } = await open();
    try {
      const { address } = await solana.getAddress(path, args.includes("--display"));
      const pub = Buffer.from(address);
      console.log(JSON.stringify({ path, pub: pub.toString("hex"), solanaAddress: base58(pub) }));
    } finally { await transport.close(); }
  } else if (cmd === "sign") {
    const file = opt("payload");
    if (!file) fail("--payload <file> is required");
    const path = checkPath(opt("path", "44'/501'/0'"));
    const envelope = readFileSync(file);
    const { text, signerPub } = parseEnvelope(envelope);
    console.error(`About to ask the Ledger to sign, as key ${signerPub}:\n  ${text}\nApprove on the device ONLY if its text (and sha256, page by page) matches the line printed by root-ceremony.mjs external-payload.`);
    const { transport, solana } = await open();
    try {
      const r = await signEnvelope(solana, path, envelope);
      const out = JSON.stringify({ path, pub: r.pub, sigBase64: r.sigBase64, sigHex: r.sigHex });
      if (opt("out")) writeFileSync(opt("out"), `${out}\n`);
      console.log(out);
    } finally { await transport.close(); }
  } else {
    fail("usage: ledger-sign.mjs <check|pubkey|sign> ... (see the header of this file)");
  }
};
run().catch((e) => fail(explain(e)));
