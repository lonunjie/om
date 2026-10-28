export function pollutedKeys(env: Record<string, string | undefined>): string[];
export function cleanBuildEnv(env: Record<string, string | undefined>, opts?: { dev?: boolean; release?: boolean }): Record<string, string>;
