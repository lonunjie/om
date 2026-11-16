import type { ProtectionCopy } from "../copy";

export const testProtectionCopy: ProtectionCopy = {
  heading: "Protection", intro: "Same encryption in all three.", scope: "Changes one thing today.", loading: "Loading protection", loadFailed: "Could not load",
  levelsLabel: "Protection levels",
  level: {
    recommended: { name: "Recommended", body: "Accepts a recovery by itself." },
    hardened: { name: "Hardened", body: "Asks you first." },
    maximum: { name: "Maximum Privacy", body: "Asks you first, like Hardened." },
  },
  custom: "Shown as Custom.", inUse: "In use", use: (n) => `Use ${n}`,
  cost: { hardened: "You will be asked.", maximum: "You will be asked." },
  loosen: { title: (n) => `Use ${n} instead?`, body: { recommended: "Accepts without asking you. Less careful.", hardened: "Less careful than now.", maximum: "Less careful than now." }, confirm: (n) => `Yes, use ${n}`, keep: "Keep what I have now" },
  applied: (n) => `Now using ${n}.`, refused: "Nothing changed.",
};
