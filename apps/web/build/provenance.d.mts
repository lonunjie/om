import type { DelegationTrust } from "./sign.mjs";
export { PROVENANCE_FILE, DSSE_PAYLOAD_TYPE, dssePae, verifyProvenance } from "./verify-release.mjs";
export declare const BUILD_TYPE: string;
export declare const DEFAULT_REPO: string;
export declare const DEFAULT_BUILDER_ID: string;
export interface Subject { name: string; digest: { sha256: string } }
export interface ProvenanceStatement {
  _type: string; subject: Subject[]; predicateType: string;
  predicate: {
    buildDefinition: { buildType: string; externalParameters: { repo: string; commit: string; version: string }; resolvedDependencies: { uri: string; name?: string; digest: Record<string, string> }[] };
    runDetails: { builder: { id: string }; metadata: { finishedOn: string } };
  };
}
export interface DsseEnvelope { payloadType: string; payload: string; signatures: { keyid: string; sig: string }[]; delegation: unknown }
export declare function buildProvenance(releaseDir: string, opts: { commit: string; finishedOn: string; builderId?: string; repo?: string; repoRoot?: string }): ProvenanceStatement;
export declare function signProvenance(statement: ProvenanceStatement, pem: string, keyId: string, delegation: unknown, trust?: DelegationTrust): DsseEnvelope;
export declare function provenanceLine(envelope: DsseEnvelope): string;
