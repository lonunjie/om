// The environment a production web build is allowed to see (audit 2026-10-28, H1).
//
// `vite build` bakes any `VITE_*` variable into the bundle, and `main.tsx` picks the harness or the in-page TEST
// loopback provider from `VITE_OM_DEV_HARNESS` / `VITE_OM_E2E_CORE`. A release built from an inherited environment
// could therefore sign a test build as production. So the release script (1) refuses to run when such a variable is
// set at all, and (2) hands every child process an allowlisted copy of the environment, never `process.env` as is.

/** Names (or patterns) that may reach a build child: toolchain locations, locale, proxies and pnpm/cargo plumbing. */
const ALLOW = [
  /^(PATH|HOME|USER|LOGNAME|SHELL|TMPDIR|TEMP|TMP|TERM|TZ|LANG|CI|NO_COLOR|FORCE_COLOR|SOURCE_DATE_EPOCH)$/,
  /^LC_[A-Z_]+$/,
  /^(XDG_[A-Z_]+|PNPM_[A-Z_]+|COREPACK_[A-Z_]+|npm_config_[a-z_]+|NPM_CONFIG_[A-Z_]+)$/,
  /^(CARGO_HOME|RUSTUP_HOME|RUSTUP_TOOLCHAIN|NODE_PATH|NODE_EXTRA_CA_CERTS|SSL_CERT_FILE|SSL_CERT_DIR)$/,
  /^(HTTPS?_PROXY|NO_PROXY|https?_proxy|no_proxy)$/,
  /^(SystemRoot|COMSPEC|PATHEXT|WINDIR)$/,
];

/** A variable that selects bundle content or releases: `VITE_*` (compiled into the bundle) and any `OM_*` switch. */
const POLLUTION = /^(VITE_|OM_)/i;

/** Names of the environment variables that must not be set when producing a release. */
export function pollutedKeys(env) {
  return Object.keys(env).filter((k) => POLLUTION.test(k) && env[k] !== undefined).sort();
}

/**
 * The allowlisted environment for build children. `dev` adds `OM_BUNDLE_DEV=1` (an explicitly unsigned dev bundle);
 * `NODE_ENV` is fixed to production so an inherited value cannot change what is built.
 */
export function cleanBuildEnv(env, { dev = false } = {}) {
  const out = {};
  for (const [k, v] of Object.entries(env)) {
    if (v !== undefined && ALLOW.some((re) => re.test(k)) && !POLLUTION.test(k)) out[k] = v;
  }
  out.NODE_ENV = "production";
  if (dev) out.OM_BUNDLE_DEV = "1";
  return out;
}
