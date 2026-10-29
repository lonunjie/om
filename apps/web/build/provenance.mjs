// Release provenance (PR-005 criterion 4, ADR-0183 section 3): an in-toto Statement v1 with a SLSA v1 provenance predicate, wrapped in a DSSE
// envelope and signed by the SAME delegated Ed25519 release key that signs the bundle manifest. No new primitive, no new dependency.
// The envelope carries the manifest's embedded root-signed delegation, so one file proves "pinned root -> delegation -> release key -> statement".
// Honest scope: the builder is the operator's own machine (self-attested provenance, SLSA build level 1-2 style); it is not an isolated hosted build.
// Subjects are site/bundle-manifest.json and SHA256SUMS. Because SHA256SUMS is a subject, provenance.intoto.jsonl is NOT listed in SHA256SUMS
// (that would be circular); the envelope authenticates itself by its signature.
import { createHash, createPrivateKey, sign } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalJson, requireDelegation } from "./sign.mjs";
import { DSSE_PAYLOAD_TYPE, PROVENANCE_FILE, SLSA_PREDICATE_TYPE, STATEMENT_TYPE, dssePae, verifyProvenance } from "./verify-release.mjs";

export { PROVENANCE_FILE, DSSE_PAYLOAD_TYPE, dssePae, verifyProvenance };
export const BUILD_TYPE = "https://lonunjie.org/om/release/v1";
export const DEFAULT_REPO = "https://github.com/lonunjie/om";
export const DEFAULT_BUILDER_ID = "https://lonunjie.org/om/release/builder/operator-workstation/v1";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const defaultRepoRoot = resolve(fileURLToPath(new URL("../../..", import.meta.url)));

/**
 * The unsigned statement for the release folder `releaseDir`.
 * `repoRoot` supplies pnpm-lock.yaml and Cargo.lock (default: this checkout). Throws if either lockfile or a subject is missing.
 */
export const buildProvenance = (releaseDir, { commit, builderId = DEFAULT_BUILDER_ID, finishedOn, repo = DEFAULT_REPO, repoRoot = defaultRepoRoot } = {}) => {
  if (typeof commit !== "string" || !/^[0-9a-f]{7,64}$/.test(commit)) throw new Error("provenance: commit must be a hex git commit id");
  if (typeof finishedOn !== "string" || Number.isNaN(Date.parse(finishedOn))) throw new Error("provenance: finishedOn must be an ISO timestamp");
  const manifest = JSON.parse(readFileSync(join(releaseDir, "site", "bundle-manifest.json"), "utf8"));
  const subject = ["site/bundle-manifest.json", "SHA256SUMS"].map((name) => ({ name, digest: { sha256: sha(readFileSync(join(releaseDir, name))) } }));
  const lock = (name) => {
    const p = join(repoRoot, name);
    if (!existsSync(p)) throw new Error(`provenance: ${name} not found in ${repoRoot}`);
    return { uri: `${repo}/blob/${commit}/${name}`, name, digest: { sha256: sha(readFileSync(p)) } };
  };
  return {
    _type: STATEMENT_TYPE,
    subject,
    predicateType: SLSA_PREDICATE_TYPE,
    predicate: {
      buildDefinition: {
        buildType: BUILD_TYPE,
        externalParameters: { repo, commit, version: manifest.version },
        resolvedDependencies: [{ uri: `git+${repo}@${commit}`, digest: { gitCommit: commit } }, lock("pnpm-lock.yaml"), lock("Cargo.lock")],
      },
      runDetails: { builder: { id: builderId }, metadata: { finishedOn } },
    },
  };
};

/**
 * DSSE envelope over the canonical-JSON statement, signed with the release key. REFUSES (throws) unless sign.mjs requireDelegation passes for
 * `delegation` (the manifest's embedded one), `pem` and `keyId` under `trust`. Returns the envelope object (one JSONL line when stringified).
 */
export const signProvenance = (statement, pem, keyId, delegation, trust = {}) => {
  const key = createPrivateKey(pem);
  if (key.asymmetricKeyType !== "ed25519") throw new Error("signing key is not ed25519");
  const d = requireDelegation(delegation, pem, keyId, trust);
  const body = Buffer.from(canonicalJson(statement), "utf8");
  return {
    payloadType: DSSE_PAYLOAD_TYPE,
    payload: body.toString("base64"),
    signatures: [{ keyid: keyId, sig: sign(null, dssePae(DSSE_PAYLOAD_TYPE, body), key).toString("base64") }],
    delegation: d,
  };
};

export const provenanceLine = (envelope) => JSON.stringify(envelope) + "\n";
