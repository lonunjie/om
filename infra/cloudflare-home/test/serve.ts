// SPDX-License-Identifier: MPL-2.0
// Runs the reference Home locally on a fixed port for the browser end-to-end test: the same Worker bundle a deploy would
// upload, in workerd, with in-memory Durable Objects. `node serve.ts [port]`. Stops on SIGINT/SIGTERM.
import { startHome } from "./harness.ts";

const port = Number(process.argv[2] ?? "8788");
const home = await startHome({ port });
process.stdout.write(`om reference Home listening on ${home.url}\n`);
const stop = (): void => { void home.dispose().finally(() => { process.exit(0); }); };
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
