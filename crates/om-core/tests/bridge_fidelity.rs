// SPDX-License-Identifier: MPL-2.0
//! `bridges` slice 4: every degraded translation carries a flag the UI can show, unknown targets never attach to an
//! unrelated message, deletes never claim erasure.
use om_core::bridge_fidelity::*;

fn full() -> Support {
    Support {
        semantics: (1..=9).collect(),
        max_attachment_bytes: Some(1000),
    }
}
fn text_only() -> Support {
    Support {
        semantics: [sem::TEXT].into(),
        max_attachment_bytes: None,
    }
}

#[test]
fn a_full_adapter_carries_everything_exactly() {
    let s = full();
    for op in [
        Op::Text { rich: true },
        Op::Attachment { bytes: 10 },
        Op::Reply {
            target_mapped: true,
        },
        Op::Edit {
            source_tracked: true,
            policy_allows: true,
        },
        Op::Delete,
        Op::Reaction {
            target_mapped: true,
        },
        Op::Topic {
            compatible_target: true,
        },
        Op::Poll {
            compatible_privacy: true,
        },
    ] {
        let p = s.translate(op);
        assert_eq!(
            (p.carry, p.flags.as_slice()),
            (Carry::Native, &[EXACT][..]),
            "{op:?}"
        );
    }
}

#[test]
fn a_text_only_adapter_degrades_visibly() {
    let s = text_only();
    assert_eq!(s.translate(Op::Text { rich: true }).carry, Carry::PlainText);
    assert_eq!(
        s.translate(Op::Text { rich: true }).flags,
        vec![LOSSY_FORMATTING]
    );
    assert_eq!(
        s.translate(Op::Attachment { bytes: 1 }).carry,
        Carry::NotCarried
    );
    assert_eq!(
        s.translate(Op::Reaction {
            target_mapped: true
        })
        .flags,
        vec![REACTION_NOT_SUPPORTED]
    );
    assert_eq!(s.translate(Op::Delete).flags, vec![DELETE_NOT_PROPAGATED]);
    assert_eq!(
        s.translate(Op::Topic {
            compatible_target: true
        })
        .carry,
        Carry::Flattened
    );
    assert_eq!(
        s.translate(Op::Poll {
            compatible_privacy: true
        })
        .carry,
        Carry::ReadOnlySummary
    );
}

#[test]
fn an_unmapped_target_never_attaches_to_an_unrelated_message() {
    let s = full();
    let r = s.translate(Op::Reply {
        target_mapped: false,
    });
    assert_eq!(
        (r.carry, r.flags),
        (Carry::UnlinkedMessage, vec![REPLY_TARGET_UNAVAILABLE])
    );
    let x = s.translate(Op::Reaction {
        target_mapped: false,
    });
    assert_eq!(
        (x.carry, x.flags),
        (Carry::NotCarried, vec![REPLY_TARGET_UNAVAILABLE])
    );
}

#[test]
fn an_edit_is_native_only_with_a_tracked_source_and_policy() {
    let s = full();
    for (t, p) in [(false, true), (true, false), (false, false)] {
        let e = s.translate(Op::Edit {
            source_tracked: t,
            policy_allows: p,
        });
        assert_eq!(
            (e.carry, e.flags),
            (Carry::NewEditedMessage, vec![EDIT_AS_NEW_MESSAGE])
        );
    }
    assert_eq!(
        text_only()
            .translate(Op::Edit {
                source_tracked: true,
                policy_allows: true
            })
            .carry,
        Carry::NewEditedMessage
    );
}

#[test]
fn an_oversized_attachment_is_flagged_transcoded() {
    assert_eq!(
        full().translate(Op::Attachment { bytes: 5000 }).flags,
        vec![ATTACHMENT_TRANSCODED]
    );
}

#[test]
fn a_delete_never_claims_erasure() {
    assert!(!full().translate(Op::Delete).claims_erasure);
    assert!(!text_only().translate(Op::Delete).claims_erasure);
}

#[test]
fn every_plan_has_a_valid_flag_set_and_provenance_flags_drop_exact() {
    for s in [full(), text_only()] {
        for op in [
            Op::Text { rich: false },
            Op::Attachment { bytes: 99999 },
            Op::Reply {
                target_mapped: false,
            },
            Op::Edit {
                source_tracked: false,
                policy_allows: true,
            },
            Op::Delete,
            Op::Reaction {
                target_mapped: true,
            },
            Op::Topic {
                compatible_target: false,
            },
            Op::Poll {
                compatible_privacy: false,
            },
        ] {
            assert!(flags_valid(&s.translate(op).flags), "{op:?}");
        }
    }
    assert_eq!(with_provenance(vec![EXACT], true, true), vec![EXACT]);
    assert_eq!(
        with_provenance(vec![EXACT], false, false),
        vec![FOREIGN_ACTOR_UNVERIFIED, ORIGIN_UNVERIFIED]
    );
    assert!(
        flags_valid(&[EXACT])
            && !flags_valid(&[])
            && !flags_valid(&[EXACT, LOSSY_FORMATTING])
            && !flags_valid(&[3, 2])
    );
}
