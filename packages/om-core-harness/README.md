# @om/core-harness (DEV/TEST ONLY)

In-memory stand-in for om-core. It contains no real crypto and is not a security boundary.
Marker: `HARNESS_MARKER = "OM-DEV-HARNESS-NOT-FOR-PRODUCTION"`; `createHarness()` throws when `import.meta.env.PROD` is true.
Import it from tests and dev entries only, never from the `apps/web` production entry or `packages/om-ui` source.
[`tools/web_guard.py`](../../tools/web_guard.py) enforces this; do not add the package to a production bundle.

Devices, device links and recovery are simulated in [`src/devices.ts`](src/devices.ts) (seed with `SeedProfile.account`).
Faults: `injectNetworkTimeoutAfterSend`, `injectUserActionRequired`; pending results finish with `resolveReconciliation` or
`resolveAuthorization`; `simulateNewDeviceOffer` stands in for a second device's link offer. None of this is an
AccountControlState implementation.
