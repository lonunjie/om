interface ImportMetaEnv {
  /** "1" only for `pnpm build:harness` and dev; selects the dev-only Core harness entry. */
  readonly VITE_OM_DEV_HARNESS?: string;
  /** "1" only for `pnpm build:core-e2e`; selects the real Core with the in-page TEST loopback provider. */
  readonly VITE_OM_E2E_CORE?: string;
}
