# OM Plugin, App and Extension Architecture

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:18 PHT
- **Scope:** protocol extensions, local client plugins, mini-app/web surfaces, provider adapters, permissions, package signing, sandboxing, update continuity and anti-fragmentation

## Decision summary

"Extension" is not one thing. OM separates six extension classes because they have different security properties:

```text
1. Protocol extension       published wire/schema semantics; no executable code implied
2. Content bundle          inert data such as sticker/content/theme packs
3. Local client plugin     executable code in a capability sandbox
4. Web/mini-app surface    remote-origin UI isolated from OM; explicit bridge only
5. Remote service/bot      separate ServicePrincipal; code does not run as client plugin
6. Provider adapter        narrow relay/storage/media/etc. capability implementation
```

An item may use more than one class, but crossing a boundary is explicit. A content pack cannot smuggle code. A protocol extension cannot automatically install code. A web app cannot automatically read the current Room. A bot cannot acquire client-plugin privileges merely because it has a command.

## Protocol extensions are schemas, not plugins

Protocol extension rules remain those in [`PROTOCOL_EXTENSIBILITY.md`](../../constitution/PROTOCOL_EXTENSIBILITY.md):

- public/namespaced extension identifier;
- explicit criticality;
- authenticated capability negotiation where required;
- deterministic schema/encoding for shared state;
- safe behavior when unknown;
- conformance artifacts for standardized semantics.

If an extension changes canonical shared conversation/control meaning, it **must** be specified at the protocol layer. Installing a proprietary plugin on two clients is not a substitute for a documented wire extension.

A plugin may help render/author an extension, but the extension's meaning cannot be defined only by opaque executable code.

## Content bundles stay inert

Sticker/content/theme packs are declarative data. They never receive:

- message plaintext APIs;
- network APIs;
- filesystem APIs;
- protocol hooks;
- code execution;
- DeviceVault/key access.

Executable functionality requires a separately installed local plugin/service/web-app surface with its own permission flow.

## Local client plugin execution format

The selected baseline direction for executable first-party-compatible plugins is the **WebAssembly Component Model** with an OM-defined capability-oriented interface world.

Conceptually:

```text
OmPluginComponent
  imports only capabilities explicitly granted by host
  exports declared extension entrypoints
  no ambient authority
  no shared OM process memory
  no direct access to OM database/keys/DOM
```

OM targets a versioned `om:plugin` WIT/component interface. The exact runtime implementation may differ by platform.

WASI is not inherited wholesale. A plugin starts with no ambient filesystem, network, environment, process or secret-store authority. The host may expose selected reviewed WASI/OM interfaces only when granted.

This choice is architectural, while PWA/native runtime feasibility/parity remains a blocking proof gate (`V-010`). A failure of one particular runtime library may cause an implementation/runtime revision without permitting an arbitrary unsandboxed in-process plugin model.

### PWA: plugins run on a separate site (ADR-0129)

In a browser, a Wasm component instantiated in the ClientHost origin, or in a same-origin worker, shares a renderer process with OM Core's wasm linear memory, which holds ratchet and MLS secrets. Import restriction does not stop Spectre-class reads. "No shared OM process memory" therefore holds in the PWA only when:

1. plugins execute in a sandboxed frame or worker served from a **different registrable domain** (a different site, not only a different origin) than the ClientHost;
2. the ClientHost is cross-origin isolated (COOP `same-origin`, COEP) so the browser can place the plugin site in its own process where the platform supports site isolation;
3. every host-to-plugin message travels over a `MessageChannel` whose endpoint is bound at creation to the exact plugin origin, and the host drops any message whose origin or channel does not match;
4. where the browser cannot give process isolation (for example a platform without site isolation), the host disables plugins that hold any `content.*` capability and says so; it never falls back to same-origin execution.

Native hosts run plugins in a separate process or an equivalent memory-isolated runtime.

## Why not arbitrary JavaScript/Swift/Kotlin/Rust in-process plugins

The official client handles:

- plaintext;
- identity state;
- Room membership/control;
- recovery material;
- DeviceVault;
- security UI.

An unrestricted in-process plugin would make every plugin equivalent to the client itself and would turn "install a fun extension" into "trust this code with all OM secrets".

Independent developers who need unrestricted application control can build a full independent OM client. The official extensibility surface remains least-privilege.

## OM host capabilities

Capabilities are narrow typed interfaces, not one "plugin access" bit.

Illustrative families:

```text
ui.composer_action
ui.message_action
ui.panel
ui.settings_page

content.read_selected
content.read_current_conversation
content.search_local_scope
content.read_attachment_selected

compose.insert
compose.create_draft
conversation.send_message     # confirmed send only; see below
conversation.set_reaction
conversation.open

net.fetch_origin_scoped
storage.plugin_private
notifications.plugin_local
service.invoke
```

Security-sensitive capabilities such as `conversation.send_message`, broad plaintext history read and network egress require prominent install/runtime disclosure.

**`conversation.send_message` is send-with-confirmation (ADR-0129).** The host shows the exact outgoing content and destination, and the user confirms each send. Only a send confirmed that way is authored by the AccountPrincipal (the Account-confirmed case in [`SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md`](SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md) §9.1). Extension code never emits Account-authored events on its own.

A plugin that must act without per-action confirmation (auto-reply, scheduled posts, a local LLM agent) runs as a **`local_component` ServicePrincipal**: it has its own ServiceID and ServiceControlState, its service endpoint credential is sealed in the device's DeviceVault under the plugin's own purpose label and is never readable by plugin code, and its events are authored by the ServicePrincipal (labelled as automation) or carry a `RepresentationGrant` ("Alice · via X"). Installing such a plugin is a personal service installation, so the notice rule in [`ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md`](../client/ENDPOINT_COERCION_AND_PLATFORM_LEAKAGE.md) §2 applies.

A permission to read plaintext does not imply permission to transmit it. Network egress is separate.

### Plaintext taint applies to plugins (ADR-0129, extending ADR-0124 rule 2)

ADR-0124's taint rule covers every executable extension that reads conversation plaintext: local plugins, personal-scope service installations, personal `AutomationRule`s and Room-installed services alike.

1. A plugin invocation that has read conversation plaintext through any `content.*` capability (or received it as a trigger) is **tainted** for that conversation.
2. A tainted invocation gets **no network egress** (`net.fetch_origin_scoped`, `service.invoke` or any other outbound channel) unless the conversation's policy grants `plaintext_egress`. In a Room that is the Room policy. In a 1:1 it needs the policy of **both** participants.
3. The one exception is a one-shot, user-initiated, per-action selected-context disclosure: the user picks the content, sees the destination, and confirms. That is treated like manual sharing ([`INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md`](INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md) §7).
4. Continuous or automated egress of conversation plaintext is **export** under Bridge §7. It needs the explicit bridge/export authorization there, never only the installing user's local grant.
5. Taint lasts for the plugin's invocation and anything it writes to `storage.plugin_private` from it; a plugin cannot launder tainted data through its private storage into a later untainted invocation.

V-010 tests this: a plugin with `content.read_current_conversation` and `net.fetch_origin_scoped` in a conversation without `plaintext_egress` must be blocked from egress.

A permission to draft does not imply permission to send. Sending is separate.

A permission to read one selected message does not imply current-Room/history access.

## Capabilities plugins never receive directly

The official plugin host does not expose raw:

- account/device private keys;
- MLS epoch secrets;
- DeviceVault master material;
- recovery secrets;
- update signing keys;
- unrestricted identity-control transitions;
- unrestricted Room/Space governance transitions;
- the host's arbitrary filesystem;
- arbitrary process execution;
- arbitrary native memory/FFI;
- raw DOM ownership over the OM shell;
- security-warning suppression/overlay privileges.

Where a plugin needs an action involving protected state, it calls a narrow host operation that performs normal policy/authorization checks.

## Declarative/host-owned UI surfaces

Plugins do not get arbitrary DOM/SwiftUI/Compose control over the main client.

The host may expose bounded surfaces:

- composer action/button;
- selected-message action;
- command palette entry;
- isolated panel/tab;
- plugin settings page;
- returned cards/forms rendered by OM host components.

Security-critical chrome remains host-owned. A plugin cannot draw over/hide:

- identity verification warnings;
- bridge/plaintext-recipient indicators;
- service/automation authorship labels ("Assistant", "Alice · via Assistant");
- foreign-provenance and `foreign_actor_unverified` labels;
- full-Room service and plaintext-recipient roster entries;
- device-link and connected-service notices;
- permission prompts;
- destructive confirmations;
- account recovery/device security state.

This is the same list as the reserved semantic UI in [`THEMES_FONTS_AND_PERSONALIZATION.md`](../ux/THEMES_FONTS_AND_PERSONALIZATION.md).

The baseline plugin UI contract is a declarative host-rendered component/event tree or other equally isolated cross-platform contract, not arbitrary HTML injected into OM's DOM.

## Mini-app / web-app surface

OM may launch remote web applications for richer cooperative experiences without granting them client-plugin authority.

Default:

```text
isolated origin/webview/iframe
 -> receives explicit launch payload only
 -> no OM cookies/credentials
 -> no message history
 -> no Room key access
 -> no ability to send as user
 -> no ambient current-chat identifier unless explicitly granted
```

A web app may request typed bridge operations. The host applies the same permission model as local plugins and may require user action/confirmation.

Because remote-origin code can change without a local package update, its origin/service identity and granted permissions must remain visible. A remote web app is never trusted merely because a previous version was reviewed.

Isolation rules (ADR-0129):

1. A mini-app is served from a **different site** than the ClientHost and runs in a sandboxed frame or webview with cross-origin isolation, for the same Spectre reason as plugins.
2. It is framed in host-owned chrome that always shows its origin and service identity. The frame cannot cover, resize over or occlude that chrome or any reserved security surface.
3. Every bridge message is bound to the exact origin the host launched; the host drops messages from any other origin or channel.
4. A mini-app may not draw content that imitates OM security prompts (recovery key entry, passkey or biometric dialogs, device-link or permission prompts). Host prompts for a mini-app's requests render outside the frame, in host chrome, never inside it.
5. The taint rule above applies: a mini-app that received conversation plaintext through the bridge is tainted.

## Plugin package and publisher continuity

Executable plugins use immutable package releases plus signed update continuity.

Conceptually:

```text
PluginID                      stable random 256-bit ID
PluginControlState            publisher/update governance
PluginPackageRelease          immutable package + manifest
PluginReleaseID               content hash of canonical package metadata
```

`PluginPackageManifest` includes:

```text
plugin_id
release_sequence/version
component_hashes
om_plugin_abi_versions[]
entrypoints/surfaces
requested_capabilities[]
network-origin requests[]
resource limits
publisher/provenance metadata
source/homepage refs?
```

Executable package releases **must** be authenticated by the current plugin control/publisher state. Self-published/self-signed plugins remain possible; "signed" means update/provenance continuity, not an OM safety endorsement.

## Permission upgrades never ride silently with code updates

If a new plugin release requests capabilities beyond the currently granted set:

- existing grants remain the ceiling;
- new capabilities stay disabled;
- the host requires explicit approval before activating them.

A package update cannot silently turn a local formatting plugin into a Room-history + Internet exfiltration plugin.

Hosts maintain rollback protection for accepted plugin-control/update state, while users/admins may intentionally pin or roll back to a known package under explicit local policy.

## Plugin resource isolation

Hosts impose per-plugin bounds such as:

- memory;
- CPU/fuel/time;
- concurrent tasks;
- local storage quota;
- network byte/request budget where granted;
- background execution duration;
- event subscription rate.

A crashed/hung plugin is terminated without compromising canonical OM state.

Canonical state changes occur only through transactional host APIs that validate normal permissions and event schemas.

## Provider adapters

Relay/storage/media/discovery/preview/etc. provider implementations remain behind existing narrow provider interfaces.

A distribution may ship built-in adapters or support dynamically installed provider components/out-of-process helpers. Dynamic adapters should use the same least-authority philosophy:

- provider-specific credentials scoped to that adapter;
- only the data required by the provider contract;
- ciphertext rather than plaintext where the provider role does not require plaintext;
- no DeviceVault/root-key access;
- no ability to redefine provider-independent IDs/canonical history.

The exact executable packaging of provider adapters need not be identical to UI plugins, but the security contract is the same: narrow capabilities, replaceable implementation, no protocol privilege.

## Remote services are not client plugins

A bot/LLM/service running elsewhere is a `ServicePrincipal`, not downloaded code.

It receives only the data authorized by [`SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md`](SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md) and cannot invoke client-plugin APIs unless the user separately installs a local companion plugin.

This split makes remote disclosure visible and prevents "install bot" from becoming "execute vendor code inside your secret-holding client."

## Extension catalogs are replaceable

OM can support independent catalogs for:

- plugins;
- content packs;
- services/bots;
- themes;
- provider adapters.

No catalog is mandatory. Direct install/share remains possible where platform policy permits.

Catalogs may attach signatures, malware analysis, reviews, ratings, age/policy classifications and compatibility metadata. Those are attestations from that catalog, not global OM truth.

The OM Project may operate a recommended catalog for its own distribution in the future, but doing so grants no wire/protocol privilege and cannot be required by independent clients.

## Anti-fragmentation rules

1. Baseline OM semantics cannot be redefined by a plugin.
2. Shared-state semantics require a published protocol extension/schema, not opaque plugin behavior.
3. Unknown critical extension semantics fail safely.
4. Optional plugin absence must not corrupt baseline canonical state.
5. A room/service may require an optional capability for a particular feature, but clients must clearly surface incompatibility instead of silently approximating it.
6. Canonical OM must not make a closed/proprietary extension mandatory.
7. Public extension IDs/registries are mirrorable; experimental extensions can use collision-resistant private namespaces without asking permission.
8. Extension payloads that are safe to preserve survive export/import even when the current client cannot render them.
9. Widely adopted extensions can be standardized without assigning special rights to the vendor that popularized them.
10. Official-client-only APIs never become unwritten protocol requirements.

## Security invariants

- no ambient authority;
- no arbitrary in-process official-client plugin model;
- plaintext read and network egress are separate grants;
- tainted invocations get no egress without conversation-level `plaintext_egress`; continuous egress is export (ADR-0129);
- no autonomous Account-authored events from extension code; autonomy means a `local_component` ServicePrincipal (ADR-0129);
- in the PWA, plugins and mini-apps run on a separate site with cross-origin isolation (ADR-0129);
- security UI cannot be overridden;
- new permissions require new consent;
- package signatures mean provenance/update continuity, not trust;
- remote code/data/principals stay visibly separate;
- provider/plugin/catalog success cannot become protocol ownership.

## Remote-origin plaintext is egress

Host taint/network-capability enforcement applies only where OM controls the execution capability boundary. A remote
mini-app origin has ambient network authority of its own; therefore **sending conversation plaintext into that origin
is itself egress**. Remote frames receive no plaintext by default. Plaintext crosses only through a one-shot
user-confirmed selected-context disclosure or an explicit conversation-authorized service/recipient path visible to
all affected participants. OM never claims that withholding a host `fetch` capability can contain plaintext already
given to a foreign origin.

Continuous plaintext egress in a 1:1 additionally requires signed `RelationshipPolicyConsent` from both participants.
The effective policy is the intersection of their newest valid consents; either side can revoke unilaterally and
absence/staleness fails closed.
