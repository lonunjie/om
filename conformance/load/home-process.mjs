// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs the SQLite Home from a built bundle as its own process, so a load run can read its memory and kill it. Not a product entry point.
// Usage: node home-process.mjs <bundle.mjs> <data dir> <vars json>
const [bundle, directory, varsJson] = process.argv.slice(2);
process.umask(0o077);
const { startHome } = await import(bundle);
const home = await startHome({ directory, hostname: "127.0.0.1", port: 0, vars: JSON.parse(varsJson ?? "{}") });
process.stdout.write(`LISTENING ${home.url}\n`);
const stop = () => { void home.dispose().finally(() => process.exit(0)); };
process.once("SIGTERM", stop);
process.once("SIGINT", stop);
