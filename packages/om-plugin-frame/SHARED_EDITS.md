# Shared integration edits

This handoff adds files under `packages/om-plugin-frame/` only. The lead owns shared changes and the commit.

| Shared path                     | Integration                                                                                                                                                                                                     |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pnpm-workspace.yaml`           | The existing `packages/*` glob includes this package. Keep the file as-is.                                                                                                                                      |
| `pnpm-lock.yaml`                | Generate the new `packages/om-plugin-frame` importer using pnpm 12.11.2. Reuse the existing exact Playwright 1.63.0, esbuild 0.27.3 and TypeScript 6.0.3 resolutions and the workspace link to `@om/core-wasm`. |
| [`machine/file_registry.json`](../../machine/file_registry.json)    | Register the new source and documentation files in this directory. Generated `dist/` output and local `results/*.json` are excluded.                                                                            |
| [`machine/architecture_map.json`](../../machine/architecture_map.json) | The design gate currently has no path mapping for the new package. If the lead adds a mapping, use the existing plugin and client extension sources and their applicable ADRs.                                  |

The package consumes the existing scope-engines source entry because its subpath is absent from the WASM package export
map. The package owns its own typecheck and browser commands. Release and app build configuration require no
integration.
