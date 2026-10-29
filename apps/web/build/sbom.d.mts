export declare function normalizeSbom(doc: Record<string, unknown>): Record<string, unknown>;
export declare function writeSbom(outFile: string, opts?: { cwd?: string; env?: NodeJS.ProcessEnv }): number;
