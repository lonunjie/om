// SPDX-License-Identifier: MPL-2.0
//! `services_bots` slice 3: nothing but the selected items leaves; partial frames never become events; cancel stops
//! future frames; the final result is a service post or a user-confirmed draft.
use om_core::service_control::*;
use om_core::service_invocation::*;
use std::collections::BTreeMap;

const SVC: [u8; 32] = [0x41; 32];
const EP: [u8; 16] = [0x10; 16];

fn chain(state: u64) -> ServiceChain {
    let mut c = ServiceChain::new(SVC);
    let eps: BTreeMap<Vec<u8>, Endpoint> = [(
        EP.to_vec(),
        Endpoint {
            id: EP.to_vec(),
            state,
            profile: 1,
            key: vec![1; 40],
            mode: 2,
            host_ref: None,
        },
    )]
    .into();
    c.accept(
        ServiceState {
            service_id: SVC,
            version: 1,
            prev: None,
            controllers: [(1u64, vec![1u8; 32])].into(),
            threshold: 1,
            endpoints: eps,
            hash: [9; 32],
        },
        &[(1, vec![1; 32])],
    )
    .unwrap();
    c
}
fn sel() -> Selection {
    Selection::new(vec![Item {
        kind: ItemKind::Message,
        bytes: b"hello".to_vec(),
    }])
    .unwrap()
}
fn req(selection: Selection) -> Request<'static> {
    Request {
        invocation: [7; 16],
        endpoint: &EP,
        selection,
        scope_hint: None,
        participants: None,
        purpose: "translate",
        expires_at: 100,
        now: 10,
    }
}

#[test]
fn only_the_selected_items_leave_and_scope_is_omitted_by_default() {
    let mut rc = Receipts::default();
    let b = disclose(&chain(ACTIVE), SVC, req(sel()), &mut rc).unwrap();
    assert_eq!(b.items.len(), 1);
    assert_eq!(b.scope_hint, None);
    assert_eq!(b.participants, None);
    let r = &rc.all()[0];
    assert_eq!(
        (r.total_bytes, r.scope_disclosed, r.participants_disclosed),
        (5, false, 0)
    );
    assert!(
        !format!("{r:?}").contains("104"),
        "receipt carries no content bytes"
    );
}

#[test]
fn scope_and_participants_are_disclosed_only_when_named() {
    let mut rc = Receipts::default();
    let mut r = req(sel());
    r.scope_hint = Some(vec![5; 32]);
    r.participants = Some(vec![vec![1], vec![2]]);
    let b = disclose(&chain(ACTIVE), SVC, r, &mut rc).unwrap();
    assert!(b.scope_hint.is_some());
    assert_eq!(
        (
            rc.all()[0].scope_disclosed,
            rc.all()[0].participants_disclosed
        ),
        (true, 2)
    );
    assert_eq!(receipts_by_service(&rc).get(&SVC), Some(&1));
}

#[test]
fn selection_bounds_and_endpoint_and_expiry_fail_closed() {
    assert_eq!(Selection::new(vec![]), Err(InvokeError::EmptySelection));
    assert_eq!(
        Selection::new(vec![Item {
            kind: ItemKind::Text,
            bytes: vec![]
        }]),
        Err(InvokeError::EmptyItem)
    );
    assert_eq!(
        Selection::new(vec![Item {
            kind: ItemKind::Text,
            bytes: vec![1; MAX_ITEM_BYTES + 1]
        }]),
        Err(InvokeError::ItemTooLarge)
    );
    let many = (0..=MAX_ITEMS)
        .map(|_| Item {
            kind: ItemKind::Text,
            bytes: vec![1],
        })
        .collect();
    assert_eq!(Selection::new(many), Err(InvokeError::TooManyItems));
    let mut rc = Receipts::default();
    assert_eq!(
        disclose(&chain(REVOKED), SVC, req(sel()), &mut rc),
        Err(InvokeError::EndpointNotUsable)
    );
    assert_eq!(
        disclose(&chain(QUARANTINED), SVC, req(sel()), &mut rc),
        Err(InvokeError::EndpointNotUsable)
    );
    let mut r = req(sel());
    r.expires_at = 10;
    assert_eq!(
        disclose(&chain(ACTIVE), SVC, r, &mut rc),
        Err(InvokeError::Expired)
    );
    let mut r = req(sel());
    r.purpose = "";
    assert_eq!(
        disclose(&chain(ACTIVE), SVC, r, &mut rc),
        Err(InvokeError::BadPurpose)
    );
    assert!(
        rc.all().is_empty(),
        "a refused disclosure writes no receipt and sends nothing"
    );
}

fn started() -> Invocation {
    let mut rc = Receipts::default();
    let b = disclose(&chain(ACTIVE), SVC, req(sel()), &mut rc).unwrap();
    Invocation::start(&b, &EP)
}

#[test]
fn frames_are_ordered_endpoint_bound_and_stop_after_cancel() {
    let mut i = started();
    i.frame(
        &EP,
        Frame {
            seq: 1,
            text: b"par".to_vec(),
        },
        20,
    )
    .unwrap();
    assert_eq!(
        i.frame(
            &EP,
            Frame {
                seq: 1,
                text: vec![]
            },
            20
        ),
        Err(FrameError::OutOfOrder)
    );
    assert_eq!(
        i.frame(
            &[0x99; 16],
            Frame {
                seq: 2,
                text: vec![]
            },
            20
        ),
        Err(FrameError::WrongEndpoint)
    );
    i.frame(
        &EP,
        Frame {
            seq: 2,
            text: b"tial".to_vec(),
        },
        21,
    )
    .unwrap();
    assert_eq!(i.frames().len(), 2);
    i.cancel();
    assert_eq!(i.phase(), Phase::Cancelled);
    assert_eq!(
        i.frame(
            &EP,
            Frame {
                seq: 3,
                text: vec![]
            },
            22
        ),
        Err(FrameError::Closed)
    );
    assert_eq!(
        i.finish(&EP, b"x".to_vec(), false, 22),
        Err(FrameError::Closed)
    );
}

#[test]
fn an_expired_invocation_accepts_nothing() {
    let mut i = started();
    assert_eq!(
        i.frame(
            &EP,
            Frame {
                seq: 1,
                text: vec![]
            },
            100
        ),
        Err(FrameError::Closed)
    );
    assert_eq!(i.phase(), Phase::Cancelled);
}

#[test]
fn the_final_result_is_a_service_post_or_a_user_confirmed_draft() {
    let mut i = started();
    i.frame(
        &EP,
        Frame {
            seq: 1,
            text: b"partial".to_vec(),
        },
        20,
    )
    .unwrap();
    let out = i.finish(&EP, b"final".to_vec(), true, 30).unwrap();
    assert_eq!(i.phase(), Phase::Finished);
    assert!(
        i.frames().is_empty(),
        "partial frames are dropped, never kept as history"
    );
    let Outcome::Draft(d) = out else {
        panic!("expected a draft")
    };
    assert_eq!(d.text(), b"final");
    // only confirm() turns it into an account-authored message, with the user's edit
    assert_eq!(
        d.confirm(b"final, edited".to_vec()),
        AccountMessage(b"final, edited".to_vec())
    );

    let mut j = started();
    assert_eq!(
        j.finish(&EP, b"reply".to_vec(), false, 30),
        Ok(Outcome::ServicePost(b"reply".to_vec()))
    );
    assert_eq!(
        j.finish(&EP, b"again".to_vec(), false, 31),
        Err(FrameError::Closed)
    );
}
