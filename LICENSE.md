# OM licensing map

OM uses multiple standard licenses because protocol specifications, shared client/core software, and hosted provider software have different anti-capture needs. See [`LICENSING_AND_CONTRIBUTION_POLICY.md`](docs/constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) for rationale.

## This planning/design repository

Unless a file contains a more specific SPDX/license notice:

- prose documentation, architecture, ADRs, research notes, design documentation and diagrams: **CC-BY-SA-4.0**;
- `spec/protocol/**`, `machine/**`, logical JSON schemas, structured JSON design/reference artifacts under `docs/ux-design/`, conformance/test-vector artifacts and architecture validation tooling: **Apache-2.0**;
- `docs/ux-design/prototype/index.html`: **Apache-2.0** as executable/reference prototype code;
- generated `MANIFEST.json`: **Apache-2.0**;
- standardized license texts in `LICENSES/` are provided under their own terms.

Historical snapshots preserve the same license classification by artifact type unless a historical file says otherwise.

## Future implementation repository/components

When product engineering is authorized:

- portable OM Core/shared security/state/client libraries: **MPL-2.0**;
- OM reference relay/directory/provider/provisioning/network-service implementations: **AGPL-3.0-or-later**;
- normative protocol/conformance/test-vector/reference artifacts: **Apache-2.0**;
- architecture and project documentation: **CC-BY-SA-4.0**.

Independent OM-compatible implementations are not required to use the reference implementation licenses merely to interoperate with the public protocol.

## Contribution rule

Normal contributions use DCO 1.1 sign-off and inbound-equals-outbound licensing. Contributors retain copyright; OM does not require broad copyright assignment as the ordinary contribution path.

Canonical protocol additions are additionally subject to the royalty-free patent-policy gate described in [`LICENSING_AND_CONTRIBUTION_POLICY.md`](docs/constitution/LICENSING_AND_CONTRIBUTION_POLICY.md) before broad external protocol contributions are accepted.

## Standard license texts

- `LICENSES/MPL-2.0.txt`
- `LICENSES/AGPL-3.0-or-later.txt`
- `LICENSES/Apache-2.0.txt`
- `LICENSES/CC-BY-SA-4.0.txt`
