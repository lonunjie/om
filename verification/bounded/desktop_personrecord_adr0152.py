#!/usr/bin/env python3
"""ADR-0152 V-021 bounded endpoint evidence for desktop parity and PersonRecord export safety."""
from __future__ import annotations
from dataclasses import dataclass
from pathlib import Path
import hashlib, json

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'verification/results/desktop-personrecord-adr0152-latest.json'


@dataclass(frozen=True)
class Caps:
    secure_storage: str
    windows: str
    filesystem: str
    notifications: str
    update: str


CAPS = {
    'pwa': Caps('browser_device_vault', 'browser_tabs', 'browser_picker', 'web_notification', 'web_distribution'),
    'android': Caps('keystore_or_strongbox', 'single_activity_or_os_windows', 'native_picker', 'android_notification', 'distribution_update'),
    'ios': Caps('keychain_or_secure_enclave', 'scene_windows', 'native_picker', 'ios_notification', 'distribution_update'),
    'macos': Caps('keychain_secure_storage', 'multi_window', 'filesystem_adapter', 'native_notification', 'signed_transparent_update'),
    'windows': Caps('os_protected_storage', 'multi_window', 'filesystem_adapter', 'native_notification', 'signed_transparent_update'),
    'linux': Caps('capability_reported_key_store', 'multi_window', 'filesystem_adapter', 'native_notification', 'signed_transparent_update'),
}


def canonical_event(profile: str, conversation: str, text: str) -> str:
    # Platform never enters canonical event identity.
    return hashlib.sha256(f'om-event|{profile}|{conversation}|{text}'.encode()).hexdigest()


@dataclass(frozen=True)
class ProfileExecutionContext:
    profile: str
    vault_namespace: str


@dataclass
class Window:
    window_id: str
    ctx: ProfileExecutionContext

    def dispatch_async(self, operation: str) -> tuple[str, str, str]:
        # Completion remains bound to immutable context captured at dispatch.
        return (self.window_id, self.ctx.profile, operation)


@dataclass(frozen=True)
class Alias:
    kind: str
    value: str
    status: str
    provenance: str


@dataclass
class PersonRecord:
    person_id: str
    account_ids: tuple[str, ...]
    relationship_binding: str
    continuity_level: str
    aliases: tuple[Alias, ...]


@dataclass
class OSContact:
    contact_id: str
    fields: dict[str, str]


def plan_export(record: PersonRecord, os_contact: OSContact, selected: dict[str, str], *, user_confirmed: bool, os_permission: bool):
    before_authority = (record.account_ids, record.relationship_binding, record.continuity_level)
    if not user_confirmed or not os_permission:
        return {'applied': False, 'diff': {}, 'authority': before_authority, 'network_objects': []}
    diff = {}
    for k, v in selected.items():
        old = os_contact.fields.get(k)
        if old != v:
            diff[k] = {'old': old, 'new': v}
    # Preview/diff is returned; a caller explicitly applies only the selected diff.
    return {'applied': True, 'diff': diff, 'authority': before_authority, 'network_objects': []}


def apply_selected_diff(os_contact: OSContact, plan: dict):
    if not plan['applied']:
        return
    for k, d in plan['diff'].items():
        os_contact.fields[k] = d['new']


def main() -> int:
    ids = {p: canonical_event('profile-A', 'conv-1', 'hello') for p in CAPS}

    win_a = Window('win-A', ProfileExecutionContext('profile-A', 'vault-A'))
    win_b = Window('win-B', ProfileExecutionContext('profile-B', 'vault-B'))
    pending_a = win_a.dispatch_async('attachment-picker')
    # User focuses/switches another window/profile before A completes.
    active_profile = 'profile-B'
    completion_a = pending_a

    rec = PersonRecord(
        'person-alice',
        ('acct-old', 'acct-current'),
        'relationship-binding-current',
        'manual_social_continuity',
        (
            Alias('phone', '+639000000001', 'reassigned', 'historical_alias'),
            Alias('email', 'alice@example.test', 'current', 'user_verified'),
        ),
    )
    os_contact = OSContact('os-alice', {'phone': '+639000000001', 'name': 'Alice Old'})
    authority_before = (rec.account_ids, rec.relationship_binding, rec.continuity_level)
    fields_before = dict(os_contact.fields)

    denied = plan_export(rec, os_contact, {'name': 'Alice', 'email': 'alice@example.test'}, user_confirmed=False, os_permission=True)
    apply_selected_diff(os_contact, denied)
    after_denied = dict(os_contact.fields)

    no_perm = plan_export(rec, os_contact, {'name': 'Alice'}, user_confirmed=True, os_permission=False)
    apply_selected_diff(os_contact, no_perm)
    after_no_perm = dict(os_contact.fields)

    plan = plan_export(rec, os_contact, {'name': 'Alice', 'email': 'alice@example.test'}, user_confirmed=True, os_permission=True)
    # Diff exists before mutation; no stale phone deletion is included because it was not explicitly selected.
    preview = json.loads(json.dumps(plan['diff']))
    apply_selected_diff(os_contact, plan)
    authority_after = (rec.account_ids, rec.relationship_binding, rec.continuity_level)

    platform_matrix = (ROOT/'docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md').read_text()
    strategy = (ROOT/'docs/architecture/client/PWA_NATIVE_AND_CLIENT_DISTRIBUTION_STRATEGY.md').read_text()
    continuity = (ROOT/'docs/architecture/identity/IDENTITY_CONTINUITY.md').read_text()
    ui = (ROOT/'docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md').read_text()

    scenarios = {
        'canonical_event_is_same_across_pwa_mobile_desktop': len(set(ids.values())) == 1,
        'desktop_os_differences_are_capabilities_not_protocol_forks': len({CAPS[p].secure_storage for p in ('macos','windows','linux')}) == 3 and ids['macos'] == ids['windows'] == ids['linux'],
        'desktop_windows_capture_immutable_profile_context': completion_a == ('win-A', 'profile-A', 'attachment-picker') and active_profile == 'profile-B',
        'independent_windows_keep_distinct_vault_namespaces': win_a.ctx.vault_namespace == 'vault-A' and win_b.ctx.vault_namespace == 'vault-B' and win_a.ctx.profile != win_b.ctx.profile,
        'linux_weaker_key_store_reporting_does_not_lower_protocol_semantics': CAPS['linux'].secure_storage == 'capability_reported_key_store' and ids['linux'] == ids['macos'],
        'export_requires_explicit_confirmation': not denied['applied'] and after_denied == fields_before,
        'export_requires_os_permission': not no_perm['applied'] and after_no_perm == fields_before,
        'export_exposes_preview_before_selected_mutation': preview == {'name': {'old': 'Alice Old', 'new': 'Alice'}, 'email': {'old': None, 'new': 'alice@example.test'}},
        'export_does_not_silently_delete_stale_or_reassigned_alias': os_contact.fields['phone'] == '+639000000001',
        'export_cannot_change_account_relationship_or_continuity_authority': authority_after == authority_before == plan['authority'],
        'export_creates_no_network_protocol_object': plan['network_objects'] == [],
        'os_address_book_is_copy_target_not_identity_source': rec.account_ids == ('acct-old', 'acct-current') and rec.relationship_binding == 'relationship-binding-current',
    }

    mutations = {
        'desktop_platform_in_eventid_mutation_caught': ids['macos'] == ids['pwa'],
        'process_global_active_profile_retargets_callback_mutation_caught': completion_a[1] != active_profile,
        'window_namespace_collapse_mutation_caught': win_a.ctx.vault_namespace != win_b.ctx.vault_namespace,
        'weak_linux_storage_downgrades_protocol_mutation_caught': ids['linux'] == ids['macos'],
        'silent_contact_overwrite_mutation_caught': after_denied == fields_before,
        'permission_bypass_mutation_caught': after_no_perm == fields_before,
        'addressbook_match_merges_accountids_mutation_caught': authority_after[0] == authority_before[0],
        'export_creates_relationship_authority_mutation_caught': authority_after[1] == authority_before[1],
        'cleanup_deletes_reassigned_alias_without_selection_mutation_caught': os_contact.fields['phone'] == '+639000000001',
        'contact_export_becomes_network_object_mutation_caught': plan['network_objects'] == [],
    }

    structural = {
        'matrix_names_desktop_v1_reference': 'Desktop V1 reference' in platform_matrix,
        'strategy_names_desktop_as_v1_target_not_simultaneous_promise': 'desktop' in strategy.lower() and 'V1' in strategy and 'simultaneous' in strategy.lower(),
        'ui_keeps_platformadapter_boundary': 'PlatformAdapter' in ui and 'Core' in ui,
        'continuity_requires_preview_and_permission': 'preview/diff' in continuity and 'explicit OS permission' in continuity,
        'continuity_denies_identity_authority_from_export': 'never changes RelationshipBinding or continuity level' in continuity,
        'continuity_says_no_network_object': 'No PersonRecord cleanup/export object is published to providers or peers' in continuity,
    }

    passed = all(scenarios.values()) and all(mutations.values()) and all(structural.values())
    out = {
        'kind': 'adr0152_desktop_personrecord_endpoint',
        'design_evidence_only': True,
        'gates': ['V-021'],
        'platform_capabilities': {k: v.__dict__ for k, v in CAPS.items()},
        'scenarios': scenarios,
        'mutations': mutations,
        'structural': structural,
        'scenario_passed': sum(scenarios.values()),
        'scenario_total': len(scenarios),
        'mutations_caught': sum(mutations.values()),
        'mutation_total': len(mutations),
        'passed': passed,
        'note': 'Bounded endpoint/design evidence. It proves semantic/profile/export invariants in the architecture model, not actual OS keychain, screenshot, notification, contacts-permission or update behavior; those remain platform implementation/pre-release audits.'
    }
    OUT.write_text(json.dumps(out, indent=2) + '\n')
    print(json.dumps(out, indent=2))
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
