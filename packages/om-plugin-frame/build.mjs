// SPDX-License-Identifier: MPL-2.0
import { build } from "esbuild";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL(".", import.meta.url));
export const config = JSON.parse(await readFile(new URL("./frame/config.json", import.meta.url), "utf8"));

export async function bundle(entryPoint, define = {}) {
  const { outputFiles } = await build({
    absWorkingDir: root,
    entryPoints: [entryPoint],
    bundle: true,
    format: "iife",
    platform: "browser",
    target: "es2023",
    define,
    write: false,
  });
  return outputFiles[0].contents;
}

export async function frameAssets(appOrigins, workerEntry = "frame/worker-entry.ts") {
  if (
    !Array.isArray(appOrigins) || !appOrigins.length || appOrigins.some((origin) => {
      const url = new URL(origin);
      return url.origin !== origin || (url.protocol !== "https:" &&
        !(url.protocol === "http:" && ["127.0.0.1", "localhost"].includes(url.hostname)));
    })
  ) {
    throw new Error("The frame requires an allowlist of exact app origins");
  }
  const [html, headers, frame, worker] = await Promise.all([
    readFile(new URL("./frame/index.html", import.meta.url)),
    readFile(new URL("./frame/_headers", import.meta.url), "utf8"),
    bundle("frame/frame.ts", { APP_ORIGINS: JSON.stringify(appOrigins) }),
    bundle(workerEntry),
  ]);
  const policy = headers.replace(/(^[^\n]*; frame-ancestors) [^\n]+$/m, `$1 ${appOrigins.join(" ")}`);
  if (!policy.includes(`; frame-ancestors ${appOrigins.join(" ")}\n`)) {
    throw new Error("Frame CSP is missing its ancestor allowlist");
  }
  return { "index.html": html, "_headers": policy, "frame.js": frame, "worker.js": worker };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const assets = await frameAssets(config.appOrigins);
  const dist = new URL("./dist/site/", import.meta.url);
  await mkdir(dist, { recursive: true });
  await Promise.all(Object.entries(assets).map(([name, bytes]) => writeFile(new URL(name, dist), bytes)));
}
