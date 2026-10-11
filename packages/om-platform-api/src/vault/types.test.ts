import { describe, expect, expectTypeOf, it } from "vitest";
import type {
  CreateVaultResult, DeleteVaultResult, DeviceVault, KvGetResult, KvListResult, OpenResult, RotateResult, SealResult,
  UnlockResult, VaultCapabilities, VaultStatus, WrappingInfoResult,
} from "./index";
import { VAULT_PURPOSES, isVaultPurpose } from "./index";

// True when T (recursively) can carry a CryptoKey or a field named like key material.
type Leaks<T> = T extends CryptoKey ? true
  : T extends Uint8Array | string | number | boolean | undefined | null ? false
  : T extends readonly (infer E)[] ? Leaks<E>
  : T extends object
    ? true extends { [K in keyof T]-?: K extends `${string}${"key" | "Key"}` | `${string}${"pvk" | "secret" | "kek"}${string}` ? true : Leaks<T[K]> }[keyof T] ? true : false
    : false;

type Results = CreateVaultResult | UnlockResult | SealResult | OpenResult | DeleteVaultResult | RotateResult | WrappingInfoResult
  | VaultCapabilities | VaultStatus | KvGetResult | KvListResult;

describe("vault types", () => {
  it("no result type can carry key material", () => {
    expectTypeOf<Leaks<Results>>().toEqualTypeOf<false>();
    expectTypeOf<Leaks<{ deviceKey: number }>>().toEqualTypeOf<true>();
    expectTypeOf<Leaks<{ a: { b: CryptoKey } }>>().toEqualTypeOf<true>();
    expectTypeOf<Awaited<ReturnType<DeviceVault["seal"]>>>().toEqualTypeOf<SealResult>();
  });

  it("purposes are a closed set", () => {
    expect(VAULT_PURPOSES).toEqual(["local-db", "search-index", "draft-store", "attachment-cache", "diagnostics-seal"]);
    expect(isVaultPurpose("local-db")).toBe(true);
    expect(isVaultPurpose("om:v1:local-db")).toBe(false);
    expect(isVaultPurpose(1)).toBe(false);
  });
});
