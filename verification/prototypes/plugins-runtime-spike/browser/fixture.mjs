// SPDX-License-Identifier: MPL-2.0
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { performance } from "node:perf_hooks";
import { gzipSync } from "node:zlib";

export const root = import.meta.dirname;
export const build = join(root, ".pwa-build");
const witFile = new URL("../../../../spec/wit/om-plugin.wit", import.meta.url);
const watFile = join(build, "fixture.wat");
const componentFile = join(build, "fixture.wasm");
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

const abiTypes = { string: "string", "list<u8>": "(list u8)" };
export const memoryPages = 4;
export const callCases = [
  { id: "init", name: "init", args: [], wave: "init()", expected: { ok: null } },
  { id: "empty-event", name: "onEvent", args: [[]], wave: "on-event([])", expected: { ok: [] } },
  { id: "event", name: "onEvent", args: [[0, 42, 255]], wave: "on-event([0, 42, 255])", expected: { ok: [0, 42, 255] } },
  { id: "render", name: "render", args: ["panel-π"], wave: 'render("panel-π")', expected: { ok: [112, 97, 110, 101, 108, 45, 207, 128] } },
  { id: "render-error", name: "render", args: [""], wave: 'render("")', expected: { err: "empty surface" } },
  { id: "grow", name: "onEvent", args: [[254]], wave: "on-event([254])", expected: { ok: [1, 3, 4, 5] } },
];

function command(program, args) {
  const start = performance.now();
  const result = spawnSync(program, args, { cwd: root, encoding: "utf8", timeout: 60_000 });
  const redact = (value) => value?.replaceAll(root + "/", "").trimEnd() ?? "";
  return { status: result.status, signal: result.signal, stdout: redact(result.stdout),
    stderr: redact(result.stderr), error: redact(result.error?.message), elapsed_ms: performance.now() - start };
}

function requireCommand(program, args) {
  const result = command(program, args);
  if (result.status !== 0) throw new Error(`${program} ${args[0]} failed: ${result.error || result.stderr}`);
  return result;
}

// Derive a no-capability instance of the WIT world from its real export declarations.
// Fail on an ABI change this deliberately small translator does not understand.
export async function makeFixture() {
  const wit = await readFile(witFile, "utf8");
  const core = await readFile(join(root, "fixture.wat"), "utf8");
  const world = wit.match(/\bworld plugin\s*\{([^}]+)\}/)?.[1];
  if (!world || !wit.includes("package om:plugin@0.1.0;")) throw new Error("Missing the frozen OM plugin world");
  const exports = world.split("\n").map((line) => line.trim()).filter((line) => line.startsWith("export "));
  const lifts = exports.map((line) => {
    const match = line.match(/^export ([\w-]+): func\(([^)]*)\) -> result<([^,]+), string>;$/);
    if (!match) throw new Error(`Unexpected WIT export: ${line}`);
    const [, name, param, success] = match;
    const input = param && param.match(/^([\w-]+): (string|list<u8>)$/);
    if ((param && !input) || (success !== "_" && !abiTypes[success])) throw new Error(`Unexpected WIT ABI: ${line}`);
    const argument = input ? `(param "${input[1]}" ${abiTypes[input[2]]})` : "";
    const result = success === "_" ? "(result (error string))" : `(result ${abiTypes[success]} (error string))`;
    return { name, wat: `(func (export "${name}") ${argument} (result ${result})
      (canon lift (core func $instance "${name}") (memory $memory) (realloc $realloc) string-encoding=utf8))` };
  });
  if (lifts.map(({ name }) => name).join() !== "init,on-event,render") throw new Error("Unexpected WIT exports");
  const wat = `(component\n${core}
    (core instance $instance (instantiate $guest))
    (alias core export $instance "memory" (core memory $memory))
    (alias core export $instance "cabi_realloc" (core func $realloc))
    ${lifts.map(({ wat }) => wat).join("\n")}\n)\n`;
  await mkdir(build, { recursive: true });
  await writeFile(watFile, wat);
  return { wit_sha256: hash(wit), fixture_wat_sha256: hash(wat) };
}

function parseWave(stdout) {
  if (stdout === "ok" || stdout === "ok()") return { ok: null };
  const match = stdout.match(/^(ok|err)\(([\s\S]+)\)$/);
  return match ? { [match[1]]: JSON.parse(match[2]) } : null;
}

export function runNative(binary = false) {
  const file = binary ? componentFile : watFile;
  const wasmtime = requireCommand("wasmtime", ["--version"]).stdout;
  const flags = ["run", "-W", `fuel=5000000,max-memory-size=${memoryPages * 65536},timeout=2s`, "--invoke"];
  const calls = Object.fromEntries(callCases.map(({ id, wave, expected }) => {
    const raw = command("wasmtime", [...flags, wave, file]);
    let result = null;
    try { result = parseWave(raw.stdout); } catch { /* Preserve unrecognized output below. */ }
    return [id, { wave, ...raw, result, pass: raw.status === 0 && JSON.stringify(result) === JSON.stringify(expected) }];
  }));
  const wave = "on-event([255])";
  const raw = command("wasmtime", [...flags, wave, file]);
  const spin = { wave, ...raw, fuel_trap: raw.status !== 0 && /fuel/.test(raw.stderr) };
  return { wasmtime, input: binary ? "component binary" : "component text",
    timing: "process wall clock including Wasmtime startup, compilation, instantiation and call",
    calls, spin, pass: Object.values(calls).every(({ pass }) => pass) && spin.fuel_trap };
}

export async function transpileFixture(expectedVersion) {
  const jco = join(root, "node_modules", ".bin", "jco");
  const version = requireCommand(jco, ["--version"]).stdout;
  if (version.replace(/^v/, "") !== expectedVersion) throw new Error(`Expected jco ${expectedVersion}, got ${version}`);
  const parsed = requireCommand(jco, ["parse", watFile, "-o", componentFile]);
  const output = join(build, "transpiled");
  await rm(output, { recursive: true, force: true }); // Only regenerable jco output.
  const probe = requireCommand(jco, ["transpile", componentFile, "--instantiation", "async",
    "--base64-cutoff=0", "--no-wasi-shim", "-o", output]);
  const component = await readFile(componentFile);
  const assets = {};
  const modules = {};
  const sizes = {};
  const files = (await readdir(output, { recursive: true, withFileTypes: true }))
    .filter((entry) => entry.isFile()).map((entry) => relative(output, join(entry.parentPath, entry.name))).sort();
  for (const name of files) {
    const bytes = await readFile(join(output, name));
    sizes[name] = { bytes: bytes.length, gzip_bytes: gzipSync(bytes).length };
    if (name.endsWith(".js")) assets[name] = bytes;
    if (name.endsWith(".wasm")) modules[name] = bytes.toString("base64");
  }
  if (!assets["fixture.js"] || !Object.keys(modules).length) throw new Error("Missing jco glue or core Wasm output");
  // Preload core bytes as a local script. The guest need not fetch its Wasm under connect-src 'none'.
  assets["fixture-bytes.js"] = Buffer.from(`export default ${JSON.stringify(modules)};\n`);
  sizes["fixture-bytes.js"] = { bytes: assets["fixture-bytes.js"].length, gzip_bytes: gzipSync(assets["fixture-bytes.js"]).length };
  const browserFiles = Object.keys(assets);
  return { assets, report: { version, parsed, transpile: probe,
    component: { bytes: component.length, sha256: hash(component), gzip_bytes: gzipSync(component).length },
    output: sizes, browser_payload_bytes: browserFiles.reduce((total, name) => total + sizes[name].bytes, 0),
    browser_payload_gzip_bytes: browserFiles.reduce((total, name) => total + sizes[name].gzip_bytes, 0) } };
}
