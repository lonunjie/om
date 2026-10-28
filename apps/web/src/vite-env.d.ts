interface ImportMetaEnv {
  /** "1" only for `pnpm build:harness` and dev; selects the dev-only Core harness entry. */
  readonly VITE_OM_DEV_HARNESS?: string;
  /** "1" only for `pnpm build:core-e2e`; selects the real Core with the in-page TEST loopback provider. */
  readonly VITE_OM_E2E_CORE?: string;
  /** Deployment default Home URL (https). Unset: the app runs unconnected. Part of the signed bundle. */
  readonly VITE_OM_RELAY_URL?: string;
  /** "1" only for `pnpm build:relay-e2e`: allows a plain-http loopback Home URL. A production build refuses it. */
  readonly VITE_OM_ALLOW_LOOPBACK_RELAY?: string;
}
