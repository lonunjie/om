export interface Sizes { readonly raw: number; readonly gzip: number }
export interface Budget { readonly baseline: { readonly deltaRawBytes: number; readonly deltaGzipBytes: number }; readonly headroomPercent: number }
export function measure(bytes: Uint8Array): Sizes;
export function limits(budget: Budget): { deltaRaw: number; deltaGzip: number };
export function checkBudget(base: Sizes, groups: Sizes, budget: Budget): { ok: boolean; delta: Sizes; limit: { deltaRaw: number; deltaGzip: number }; errors: string[] };
