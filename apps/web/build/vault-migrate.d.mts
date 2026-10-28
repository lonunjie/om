export interface KeyInfo { id: string; pub: string; fingerprint: string; value: string }
export type Derive = (input: string | object, opts?: { allowVisibleField?: boolean }) => KeyInfo;
export declare const KINDS: Record<string, { family: "ed25519" | "pq"; describe: (info: { id?: string; pub?: string; fingerprint: string }) => string }>;
export declare const KIND_NAMES: string[];
export declare function defaultItemName(kind: string): string;
export declare function planMigration(item: unknown, kind: string, derive: Derive): { info: KeyInfo; steps: Record<string, unknown>[] };
export declare function migrateViaBw(o: { itemName: string; kind: string; derive: Derive; bw?: string; dryRun?: boolean; spawn?: unknown }): { info: KeyInfo; written: number; planned: number };
