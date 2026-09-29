# UI/UX validation helpers

[`run_static_checks.py`](run_static_checks.py) performs deterministic checks against OM Clear theme tokens. It is intentionally narrow: it verifies token completeness and selected WCAG-style contrast relationships. It does **not** replace browser accessibility tests, rendering inspection, color-vision testing, usability studies, or performance benchmarks.

Run from the package root:

```sh
python3 docs/design/ui-ux/validation/run_static_checks.py
```

## Revision 10 PWA conformance

Run:

```bash
python3 run_pwa_conformance.py --json ../research/PWA_CONFORMANCE_R10.json
```

The harness tests the `pwa-reference-v2` fixture. It records unavailable browser engines as `not_run`; it does not infer AT or physical-device conformance from browser automation.

- [`validate_r13c_system.py`](validate_r13c_system.py) — validates R13C settings/layout/localization/sensory/performance/token/scope contract invariants.
