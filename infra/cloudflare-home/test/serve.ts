// SPDX-License-Identifier: AGPL-3.0-or-later
// Runs the reference Home locally on a fixed port for the browser end-to-end test: the same Worker bundle a deploy would
// upload, in workerd, with in-memory Durable Objects. `node serve.ts [port]`. Stops on SIGINT/SIGTERM.
import { startHome } from "./harness.ts";

const port = Number(process.argv[2] ?? "8788");
// OM_WAKE=0 starts the Home without the optional wake channel (om.relay.wake.v1), for before/after request counts.
// OM_ADMISSION=<hex hash(es)> starts a locked personal Home (om.admission.v1), the shape a person's own deployment has.
const vars: Record<string, string> = {};
if (process.env["OM_WAKE"] === "0") vars["OM_WAKE"] = "0";
if (process.env["OM_ADMISSION"]) vars["OM_ADMISSION"] = process.env["OM_ADMISSION"];
const home = await startHome({ port, vars });
process.stdout.write(`om reference Home listening on ${home.url}\n`);
const stop = (): void => { void home.dispose().finally(() => { process.exit(0); }); };
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
