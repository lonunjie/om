interface ImportMetaEnv {
  /** "1" only for `pnpm build:harness` and dev; selects the dev-only Core harness entry. */
  readonly VITE_OM_DEV_HARNESS?: string;
  /** "1" only for `pnpm build:core-e2e`; selects the real Core with the in-page TEST loopback provider. */
  readonly VITE_OM_E2E_CORE?: string;
  /** Dev and test bundles only (a signed build refuses it, ADR-0186): a Home URL baked into the bundle. Unset: the app runs unconnected. */
  readonly VITE_OM_RELAY_URL?: string;
  /** https address of the one-click delivery-service setup that first run links to (ADR-0186). Unset: the page says setup is not ready yet. Not a test flag; a signed build may carry it. */
  readonly VITE_OM_RELAY_DEPLOY_URL?: string;
  /** "1" only for `pnpm build:relay-e2e`: allows a plain-http loopback Home URL. A production build refuses it. */
  readonly VITE_OM_ALLOW_LOOPBACK_RELAY?: string;
}
