export const SCRIPT_BLOCKS: Readonly<Record<string, readonly (readonly [number, number])[]>>;
export const EXTRACT_PATH: string;
export const GENERATED_PATH: string;
export interface Line { readonly cp: number; readonly to: string; readonly line: string }
export function parseLines(text: string): Line[];
export function keep(p: { readonly cp: number; readonly to: string }): boolean;
export function extract(full: string): string;
export function generate(extractText: string): string;
