// SPDX-License-Identifier: MPL-2.0
// Thin typed helpers over the Durable Object SQLite API. Every statement is parameterised; nothing is concatenated.

type Bind = string | number | Uint8Array | null;

/* eslint-disable @typescript-eslint/no-unnecessary-type-parameters -- the row type is the caller's assertion about its own query */
export function all<T>(sql: SqlStorage, query: string, ...binds: Bind[]): T[] {
  return sql.exec(query, ...binds).toArray() as unknown as T[];
}

export function one<T>(sql: SqlStorage, query: string, ...binds: Bind[]): T | undefined {
  return all<T>(sql, query, ...binds)[0];
}

export function run(sql: SqlStorage, query: string, ...binds: Bind[]): void {
  sql.exec(query, ...binds);
}

export function getMeta(sql: SqlStorage, key: string): string | number | Uint8Array | null | undefined {
  return one<{ v: string | number | Uint8Array | null }>(sql, "SELECT v FROM meta WHERE k = ?", key)?.v;
}

export function setMeta(sql: SqlStorage, key: string, value: string | number | Uint8Array | null): void {
  run(sql, "INSERT INTO meta (k, v) VALUES (?, ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v", key, value);
}

export function metaNumber(sql: SqlStorage, key: string, fallback = 0): number {
  const v = getMeta(sql, key);
  return typeof v === "number" ? v : fallback;
}

export function metaString(sql: SqlStorage, key: string): string | undefined {
  const v = getMeta(sql, key);
  return typeof v === "string" ? v : undefined;
}
