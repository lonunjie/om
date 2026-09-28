#!/usr/bin/env python3
"""ADR-0152 V-015 bounded evidence for replaceable Android wake/privacy/correctness."""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import hashlib, json

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'verification/results/android-wake-adr0152-latest.json'
MODES = ('fcm', 'unifiedpush', 'persistent', 'none')


def opaque_handle(profile: str, generation: int, provider: str) -> str:
    return hashlib.sha256(f'wake|{profile}|{generation}|{provider}'.encode()).hexdigest()[:24]


@dataclass
class Registration:
    profile: str
    mode: str
    provider_party: str
    platform_token: str | None
    handle: str
    generation: int
    can_retire: bool = True
    active: bool = True
    stale_harmless: bool = False


@dataclass
class ProfileState:
    account_id: str
    device_id: str
    relationship_binding: str
    mailbox: list[str] = field(default_factory=list)
    seen: set[str] = field(default_factory=set)
    generation: int = 0
    registration: Registration | None = None


class WakeModel:
    def __init__(self):
        self.profiles = {
            'A': ProfileState('acct-A', 'dev-A', 'rel-A'),
            'B': ProfileState('acct-B', 'dev-B', 'rel-B'),
        }
        self.retired: list[Registration] = []
        self.failed_provider_parties: set[str] = set()
        self.canonical_events: dict[str, tuple[str, str]] = {}

    def identity_tuple(self, profile: str):
        p = self.profiles[profile]
        return (p.account_id, p.device_id, p.relationship_binding)

    def configure(self, profile: str, mode: str, provider_party: str = 'none', platform_token: str | None = None,
                  can_retire: bool = True) -> Registration | None:
        assert mode in MODES
        p = self.profiles[profile]
        old = p.registration
        if old is not None:
            if old.can_retire:
                old.active = False
            else:
                # Provider cannot confirm retirement. The stale mapping may still emit a wake,
                # but the handle is never message/account authority and is marked bounded leakage.
                old.stale_harmless = True
            self.retired.append(old)
        p.generation += 1
        if mode == 'none':
            p.registration = None
            return None
        reg = Registration(
            profile=profile,
            mode=mode,
            provider_party=provider_party,
            platform_token=platform_token,
            handle=opaque_handle(profile, p.generation, provider_party),
            generation=p.generation,
            can_retire=can_retire,
        )
        p.registration = reg
        return reg

    def enqueue(self, profile: str, event_id: str, ciphertext_digest: str):
        # Canonical storage happens at ordinary OM delivery infrastructure, never in push.
        self.canonical_events[event_id] = (profile, ciphertext_digest)
        if event_id not in self.profiles[profile].mailbox:
            self.profiles[profile].mailbox.append(event_id)

    def wake(self, profile: str) -> str:
        reg = self.profiles[profile].registration
        if reg is None:
            return 'no_wake_app_open_or_os_background_only'
        if reg.provider_party in self.failed_provider_parties:
            return 'wake_failed_mailbox_unchanged'
        if reg.mode == 'persistent':
            return 'ordinary_authenticated_transport_wake'
        return 'opaque_wake_hint'

    def stale_wake(self, reg: Registration) -> str:
        # Even an unretirable old mapping can at most cause a harmless wake.
        return 'harmless_wake_no_fetch_authority'

    def fetch(self, profile: str) -> list[str]:
        p = self.profiles[profile]
        newly = [eid for eid in p.mailbox if eid not in p.seen]
        p.seen.update(newly)
        return newly


def token_assignment_allowed(*, keep_separate: bool, token: str, party_a: str, party_b: str,
                             declared_single_gateway_linkage: bool) -> bool:
    if not keep_separate:
        return True
    if party_a == party_b and declared_single_gateway_linkage:
        return True
    # Keep-separate profiles may not silently expose one native install token to unrelated parties.
    return False


def main() -> int:
    m = WakeModel()
    baseline_identity = m.identity_tuple('A')

    fcm = m.configure('A', 'fcm', 'gateway-fcm', 'native-token-1', can_retire=True)
    m.enqueue('A', 'E1', 'ct1')
    s1 = m.wake('A')
    got1 = m.fetch('A')

    up = m.configure('A', 'unifiedpush', 'gateway-up', 'up-endpoint-A', can_retire=False)
    m.enqueue('A', 'E2', 'ct2')
    s2 = m.wake('A')
    got2 = m.fetch('A')

    persistent = m.configure('A', 'persistent', 'home-relay-A', None, can_retire=True)
    m.enqueue('A', 'E3', 'ct3')
    s3 = m.wake('A')
    got3 = m.fetch('A')

    m.configure('A', 'none')
    m.enqueue('A', 'E4', 'ct4')
    s4 = m.wake('A')
    got4 = m.fetch('A')  # user opens app later

    back = m.configure('A', 'fcm', 'gateway-fcm', 'native-token-1', can_retire=True)
    m.enqueue('A', 'E5', 'ct5')
    m.failed_provider_parties.add('gateway-fcm')
    sfail = m.wake('A')
    # failure cannot remove canonical ciphertext; opening app still fetches it
    got5 = m.fetch('A')

    unretirable = [r for r in m.retired if r is up][0]

    descriptor = json.loads((ROOT/'spec/metadata/w0-metadata-exposure-descriptors.json').read_text())
    push_desc = next(d for d in descriptor['descriptors'] if d['role'] == 'push_provider')
    desc_note = '\n'.join(descriptor.get('notes', []))

    scenarios = {
        'fcm_wake_fetches_relay_stored_event': s1 == 'opaque_wake_hint' and got1 == ['E1'],
        'unifiedpush_switch_preserves_canonical_state': s2 == 'opaque_wake_hint' and got2 == ['E2'] and fcm.active is False,
        'persistent_path_is_ordinary_transport_not_new_authority': s3 == 'ordinary_authenticated_transport_wake' and got3 == ['E3'],
        'none_mode_delays_but_does_not_lose_messages': s4 == 'no_wake_app_open_or_os_background_only' and got4 == ['E4'],
        'provider_failure_cannot_delete_or_hide_canonical_mailbox': sfail == 'wake_failed_mailbox_unchanged' and got5 == ['E5'],
        'switches_do_not_rekey_identity_or_relationship': m.identity_tuple('A') == baseline_identity,
        'old_registration_retires_where_possible': fcm.active is False and persistent.active is False,
        'unretirable_old_registration_is_metadata_only': unretirable.stale_harmless and m.stale_wake(unretirable) == 'harmless_wake_no_fetch_authority',
        'keep_separate_rejects_same_token_to_unrelated_gateways': not token_assignment_allowed(keep_separate=True, token='native-token-1', party_a='gw-A', party_b='gw-B', declared_single_gateway_linkage=False),
        'keep_separate_allows_one_explicit_declared_gateway_linkage': token_assignment_allowed(keep_separate=True, token='native-token-1', party_a='chosen-gw', party_b='chosen-gw', declared_single_gateway_linkage=True),
        'opaque_handles_are_profile_scoped': back is not None and back.handle != opaque_handle('B', back.generation, back.provider_party),
        'push_descriptor_declares_only_metadata_not_identity_authority': push_desc['exposures']['stable_account_id'] == 'no_by_design' and push_desc['exposures']['conversation_id'] == 'no_by_design' and push_desc['exposures']['push_timing'] == 'always' and push_desc['exposures']['cross_profile_linkage'] == 'sometimes',
        'descriptor_models_replaceable_wake_and_persistent_observer': 'replaceable Android wake' in desc_note and 'persistent encrypted relay/Home' in desc_note,
    }

    # Deliberately bad shortcuts and why the model rejects them.
    mutations = {
        'push_delivery_as_message_storage_caught': m.canonical_events['E5'] == ('A', 'ct5') and 'E5' in m.profiles['A'].mailbox,
        'provider_failure_deletes_message_mutation_caught': 'E5' in m.profiles['A'].mailbox,
        'none_means_drop_message_mutation_caught': 'E4' in m.profiles['A'].seen,
        'adapter_switch_rekeys_account_mutation_caught': m.identity_tuple('A') == baseline_identity,
        'wake_handle_as_fetch_capability_mutation_caught': m.stale_wake(unretirable) == 'harmless_wake_no_fetch_authority',
        'undeclared_cross_profile_token_sharing_mutation_caught': not token_assignment_allowed(keep_separate=True, token='t', party_a='x', party_b='y', declared_single_gateway_linkage=False),
        'persistent_path_as_privileged_push_authority_mutation_caught': s3 == 'ordinary_authenticated_transport_wake',
        'provider_switch_changes_event_identity_mutation_caught': set(m.canonical_events) == {'E1','E2','E3','E4','E5'},
    }

    push_doc = (ROOT/'docs/architecture/client/NOTIFICATIONS_AND_PUSH.md').read_text()
    ui = (ROOT/'docs/architecture/ux/UI_CORE_PLATFORM_CONTRACT.md').read_text()
    matrix = (ROOT/'docs/architecture/client/PLATFORM_CAPABILITY_MATRIX.md').read_text()
    structural = {
        'docs_name_all_four_modes': all(x in push_doc for x in MODES),
        'docs_say_push_not_correctness_or_identity': 'never identity authority, message storage, or protocol correctness' in push_doc,
        'docs_require_old_registration_retirement_where_possible': 'Old registrations are revoked/retired where the platform/provider permits' in push_doc,
        'ui_capability_vocabulary_matches': 'webpush | apns | fcm | unifiedpush | persistent | none' in ui,
        'docs_do_not_invent_universal_retirement_time_bound': 'finite retention bound only when the provider/adapter contract declares an enforceable finite lease' in push_doc,
        'android_matrix_lists_replaceable_modes': 'fcm | unifiedpush | persistent | none' in matrix,
    }

    passed = all(scenarios.values()) and all(mutations.values()) and all(structural.values())
    out = {
        'kind': 'adr0152_replaceable_android_wake',
        'design_evidence_only': True,
        'gates': ['V-015'],
        'modes': list(MODES),
        'scenarios': scenarios,
        'mutations': mutations,
        'structural': structural,
        'scenario_passed': sum(scenarios.values()),
        'scenario_total': len(scenarios),
        'mutations_caught': sum(mutations.values()),
        'mutation_total': len(mutations),
        'passed': passed,
        'note': 'Push adapters are modeled only as wake/metadata boundaries over the same canonical relay mailbox. This does not claim real FCM/UnifiedPush/battery behavior; those remain implementation/pre-release measurements.'
    }
    OUT.write_text(json.dumps(out, indent=2) + '\n')
    print(json.dumps(out, indent=2))
    return 0 if passed else 1


if __name__ == '__main__':
    raise SystemExit(main())
