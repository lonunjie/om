export declare const SECRET_FIELD: string;
export declare const LEGACY_FIELD: string;
export type SecretKind = "ed25519" | "pq";
export declare const FIELD_PREFIX: Record<SecretKind, string>;
export declare function secretFieldName(kind: SecretKind, date?: string): string;
export declare function isSecretFieldName(name: unknown, kind: SecretKind): boolean;
export interface ReadOpts { allowVisibleField?: boolean; fieldName?: string }
export declare const FIELD_HIDDEN: number;
export interface ItemField { name: string; value: string; type: number; linkedId: null }
export interface VaultItem { type: number; name: string; notes: string; secureNote: { type: number }; favorite: boolean; organizationId: null; folderId: null; fields: ItemField[] }
export declare function sha256Hex(bytes: Uint8Array): string;
export declare function locateSecret(input: string | object, opts?: ReadOpts & { kind?: SecretKind }): { value: string; strict: boolean; source: "field" | "notes" | "raw"; notes: string; fieldType?: number; fieldName?: string };
export declare function normalizeEd25519(text: string, opts?: { strict?: boolean }): string;
export declare function ed25519FieldValue(pemOrKey: unknown): string;
export declare function ed25519PublicHex(pem: string): string;
export declare function readEd25519(input: string | object, opts?: ReadOpts): { pem: string; notes: string; source: string; fieldName?: string };
export declare function normalizePqSeed(text: string, opts?: { strict?: boolean }): { seedHex: string; meta: Record<string, string> };
export declare function readPqSeed(input: string | object, opts?: ReadOpts): { seedHex: string; meta: Record<string, string>; notes: string; source: string; fieldName?: string };
export declare function describeItem(o: { purpose: string; keyId: string; keyIdLabel?: string; publicKey?: string; fingerprint: string; extra?: string[]; fieldName?: string }): string;
export declare function checkDescription(notes: string, claims: { publicKey?: string; fingerprint?: string; keyId?: string }): void;
export declare function secureNoteItem(o: { name: string; notes: string; secret: string; fieldName: string }): VaultItem;
