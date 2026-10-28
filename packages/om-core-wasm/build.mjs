// Builds crates/om-wasm for wasm32-unknown-unknown (release) and copies it to dist/om_wasm.wasm with a SHA-256
// manifest. Uses the pinned toolchain in rust-toolchain.toml; never installs anything.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../..");
const env = { ...process.env, PATH: `/opt/homebrew/opt/rustup/bin:${process.env.PATH ?? ""}` };
execFileSync("cargo", ["build", "-p", "om-wasm", "--target", "wasm32-unknown-unknown", "--release", "--locked"], { cwd: root, env, stdio: "inherit" });
mkdirSync(join(here, "dist"), { recursive: true });
const out = join(here, "dist/om_wasm.wasm");
copyFileSync(join(root, "target/wasm32-unknown-unknown/release/om_wasm.wasm"), out);
const sha256 = createHash("sha256").update(readFileSync(out)).digest("hex");
writeFileSync(join(here, "dist/om_wasm.wasm.sha256"), `${sha256}  om_wasm.wasm\n`);
console.log(`om_wasm.wasm ${sha256}`);
