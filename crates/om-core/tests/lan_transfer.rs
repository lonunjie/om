// SPDX-License-Identifier: MPL-2.0
//! `local_delivery` slice 6: a multi-MiB blob transfers with backpressure, resumes from the acknowledged offset and is
//! confirmed only by the authenticated receipt.
use om_core::lan_transfer::*;

const ID: [u8; 32] = [1; 32];
const DG: [u8; 32] = [2; 32];
const MIB: u64 = 1024 * 1024;

#[test]
fn a_multi_mib_file_transfers_with_bounded_flight_and_confirms_only_by_receipt() {
    let total = 5 * MIB + 123;
    let mut s = Sender::new(ID, DG, total);
    let mut r = Receiver::new(total);
    let mut paused = false;
    let mut peak = 0;
    let mut chunks = 0u64;
    loop {
        // the channel delivers chunks in bursts; the transport acks after each burst
        let mut burst = Vec::new();
        while let Some(c) = s.next_chunk(&mut paused) {
            burst.push(c);
            peak = peak.max(s.in_flight());
        }
        if burst.is_empty() {
            break;
        }
        for c in &burst {
            assert!(c.len <= MAX_CHUNK);
            r.accept(c).unwrap();
            chunks += 1;
        }
        s.transport_ack(r.acknowledged()).unwrap();
    }
    assert!(
        peak <= HIGH_WATER + MAX_CHUNK,
        "in flight stayed bounded: {peak}"
    );
    assert!(r.is_complete());
    assert_eq!(chunks, total.div_ceil(MAX_CHUNK as u64));
    assert!(!s.is_confirmed(), "transport acks never confirm");
    s.receipt(true, &ID, &DG, total).unwrap();
    assert!(s.is_confirmed());
}

#[test]
fn only_an_authenticated_matching_receipt_confirms() {
    let mut s = Sender::new(ID, DG, 100);
    assert_eq!(
        s.receipt(false, &ID, &DG, 100),
        Err(TransferError::ReceiptNotAuthenticated)
    );
    assert_eq!(
        s.receipt(true, &[9; 32], &DG, 100),
        Err(TransferError::ReceiptMismatch)
    );
    assert_eq!(
        s.receipt(true, &ID, &[9; 32], 100),
        Err(TransferError::ReceiptMismatch)
    );
    assert_eq!(
        s.receipt(true, &ID, &DG, 99),
        Err(TransferError::ReceiptMismatch)
    );
    assert!(!s.is_confirmed());
}

#[test]
fn resume_restarts_from_the_receivers_acknowledged_offset() {
    let total = 100_000u64;
    let mut s = Sender::new(ID, DG, total);
    let mut r = Receiver::new(total);
    let mut paused = false;
    for _ in 0..3 {
        let c = s.next_chunk(&mut paused).unwrap();
        r.accept(&c).unwrap();
    }
    let durable = r.acknowledged();
    // the link drops after a fourth chunk was sent but not stored
    let lost = s.next_chunk(&mut paused).unwrap();
    assert_eq!(lost.offset, durable);
    let mut r2 = Receiver::restore(total, durable);
    s.resume_from(r2.acknowledged()).unwrap();
    let c = s.next_chunk(&mut false).unwrap();
    assert_eq!(
        c.offset, durable,
        "resume never starts past what the receiver stored"
    );
    r2.accept(&c).unwrap();
    assert_eq!(s.resume_from(total + 1), Err(TransferError::BadAck));
}

#[test]
fn the_receiver_accepts_only_the_next_contiguous_bounded_chunk() {
    let mut r = Receiver::new(40_000);
    assert_eq!(
        r.accept(&Chunk {
            offset: 10,
            len: 100
        }),
        Err(TransferError::BadChunk)
    );
    assert_eq!(
        r.accept(&Chunk {
            offset: 0,
            len: MAX_CHUNK + 1
        }),
        Err(TransferError::BadChunk)
    );
    assert_eq!(
        r.accept(&Chunk { offset: 0, len: 0 }),
        Err(TransferError::BadChunk)
    );
    r.accept(&Chunk {
        offset: 0,
        len: MAX_CHUNK,
    })
    .unwrap();
    r.accept(&Chunk {
        offset: MAX_CHUNK as u64,
        len: MAX_CHUNK,
    })
    .unwrap();
    assert_eq!(
        r.accept(&Chunk {
            offset: 2 * MAX_CHUNK as u64,
            len: MAX_CHUNK
        }),
        Err(TransferError::Overrun)
    );
}

#[test]
fn bad_transport_acks_are_refused_and_backpressure_has_hysteresis() {
    let mut s = Sender::new(ID, DG, 10 * MIB);
    let mut paused = false;
    while s.next_chunk(&mut paused).is_some() {}
    assert!(paused && s.in_flight() >= HIGH_WATER);
    assert_eq!(s.transport_ack(u64::MAX), Err(TransferError::BadAck));
    // acking a little (still above the low mark) stays paused; acking below the low mark resumes
    let acked_small = (s.in_flight() - HIGH_WATER + MAX_CHUNK) as u64;
    s.transport_ack(acked_small).unwrap();
    assert!(s.in_flight() >= LOW_WATER && s.next_chunk(&mut paused).is_none());
    s.transport_ack(s.in_flight() as u64 + acked_small - (LOW_WATER as u64 - 1))
        .unwrap();
    assert!(s.next_chunk(&mut paused).is_some() && !paused);
    assert_eq!(s.transport_ack(0), Err(TransferError::BadAck));
}
