// Builds crates/om-wasm for wasm32-unknown-unknown (release, with om-core/production) and copies it to dist/om_wasm.wasm with a SHA-256
// manifest. Uses the pinned toolchain in rust-toolchain.toml; never installs anything.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const env = { ...process.env, PATH: `/opt/homebrew/opt/rustup/bin:${process.env.PATH ?? ""}` };
// `om-core/production` is the R-072 marker: it turns a test-only feature (insecure RNG, loopback provider) into a
// compile error, so the shipped artifact is built under that guard rather than by convention.
// `--groups` builds the variant with the OpenMLS engine (`groups-openmls`, ADR-0187, a proposed delegated draft) to
// dist/om_wasm_groups.wasm. It is never the default artifact and never what `make core-wasm` ships.
const groups = process.argv.includes("--groups");
const features = groups ? "om-core/production,groups-openmls" : "om-core/production";
execFileSync("cargo", ["build", "-p", "om-wasm", "--target", "wasm32-unknown-unknown", "--release", "--locked", "--features", features], { cwd: root, env, stdio: "inherit" });
mkdirSync(join(here, "dist"), { recursive: true });
const name = groups ? "om_wasm_groups.wasm" : "om_wasm.wasm";
const out = join(here, "dist", name);
copyFileSync(join(root, "target/wasm32-unknown-unknown/release/om_wasm.wasm"), out);
const sha256 = createHash("sha256").update(readFileSync(out)).digest("hex");
writeFileSync(join(here, `dist/${name}.sha256`), `${sha256}  ${name}\n`);
console.log(`${name} ${sha256}`);
