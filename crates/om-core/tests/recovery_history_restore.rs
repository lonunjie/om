// SPDX-License-Identifier: MPL-2.0
//! History after recovery (ADR-0184, ADR-0202 section 3): an account recovered through a surviving controller can read
//! the history of a backup file as read-only restored conversations. Restoring grants no authority: the recovered
//! account state, its device and its recovery policy are untouched, and a restored conversation cannot send.

mod common;

use common::recovery_world::{adopt_via_approval, fresh, world};
use om_core::api::*;
use om_core::backup::BackupPlan;
use om_core::host::Step;
use om_core::runtime::settle_all;

const KEY: [u8; 32] = [9; 32];

#[test]
fn a_recovered_account_reads_its_old_history_and_the_restore_changes_no_authority() {
    let mut w = world();
    // Alice and Bob talk; Alice saves a backup file before she loses her devices.
    let conv: Vec<ConversationSummary> = w
        .alice
        .query(Query::GetConversationList { ctx: w.a.clone() });
    let id = conv[0].id.clone();
    w.alice.dispatch(
        &w.a,
        "s1",
        IntentBody::SendMessage {
            conversation_id: id.clone(),
            draft: Draft {
                text: "Lunch tomorrow?".into(),
                reply_to: None,
            },
        },
    );
    settle_all(&mut [(&mut w.alice, &w.a), (&mut w.bob, &w.b)]);
    let plan: BackupPlan = w.alice.core.backup_plan();
    let file = w.alice.core.export_history(&w.a, &plan, &KEY).unwrap();

    let mut f = fresh(&w.net);
    f.begin(&w.sheet);
    adopt_via_approval(&mut w, &mut f);
    assert!(f.view().is_none(), "the account was recovered");
    let before = f.app();

    let (summary, commit) = f.rt.core.import_history(&f.ctx, &file, &KEY).unwrap();
    assert!(f.rt.settle(&Step {
        commit,
        handle: None
    }));
    assert_eq!((summary.chats_added, summary.messages_added), (1, 1));
    let list: Vec<ConversationSummary> =
        f.rt.query(Query::GetConversationList { ctx: f.ctx.clone() });
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].relationship, RelationshipStateCode::Restored);

    // Authority did not move: chain, credentials, links, device, policy and relationships are exactly as adopted.
    let after = f.app();
    for k in [
        "acsChain",
        "acsCredentials",
        "recoveryLinks",
        "recoveryPolicy",
        "relationships",
    ] {
        assert_eq!(before[k], after[k], "{k} is untouched by a restore");
    }
    assert_eq!(after["restored"].as_object().unwrap().len(), 1);
    // A restored conversation has no relationship: it cannot send.
    f.rt.dispatch(
        &f.ctx,
        "try",
        IntentBody::SendMessage {
            conversation_id: id,
            draft: Draft {
                text: "hi".into(),
                reply_to: None,
            },
        },
    );
    assert!(matches!(
        f.rt.outcome(&f.ctx, "try"),
        Some(OutcomeClass::Rejected { .. })
    ));
}
