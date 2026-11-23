// SPDX-License-Identifier: MPL-2.0
//! Fidelity and translation rules (`bridges` slice 4; INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md sections 10, 11
//! and 12, `bridge_semantic_support` and `bridge_fidelity_flag` registries). Pure planner: given what an adapter supports
//! and what the mapping table knows, it says how an operation is carried and which fidelity flags the UI must show.
//!
//! * `exact` (1) is alone: any degraded translation carries at least one other flag and never `exact`.
//! * An unknown target never attaches to an unrelated message: the reply or reaction loses its link visibly instead.
//! * An edit becomes a native edit only when the source event is tracked and the mapping policy allows it; otherwise it is
//!   a new "edited version" message. A delete never claims erasure.

pub const EXACT: u8 = 1;
pub const LOSSY_FORMATTING: u8 = 2;
pub const REPLY_TARGET_UNAVAILABLE: u8 = 3;
pub const EDIT_AS_NEW_MESSAGE: u8 = 4;
pub const DELETE_NOT_PROPAGATED: u8 = 5;
pub const REACTION_NOT_SUPPORTED: u8 = 6;
pub const ATTACHMENT_TRANSCODED: u8 = 7;
pub const FOREIGN_ACTOR_UNVERIFIED: u8 = 8;
pub const ORIGIN_UNVERIFIED: u8 = 9;

/// `bridge_semantic_support` ids.
pub mod sem {
    pub const TEXT: u8 = 1;
    pub const RICH_TEXT: u8 = 2;
    pub const ATTACHMENTS: u8 = 3;
    pub const REPLIES: u8 = 4;
    pub const EDITS: u8 = 5;
    pub const DELETES: u8 = 6;
    pub const REACTIONS: u8 = 7;
    pub const POLLS: u8 = 8;
    pub const THREADS_TOPICS: u8 = 9;
}

use std::collections::BTreeSet;

/// What the adapter advertised (`InteropCapabilities.0`) and its attachment ceiling.
pub struct Support {
    pub semantics: BTreeSet<u8>,
    pub max_attachment_bytes: Option<u64>,
}

/// What the mapping table and policy know about one operation.
#[derive(Clone, Copy, Debug)]
pub enum Op {
    Text {
        rich: bool,
    },
    Attachment {
        bytes: u64,
    },
    Reply {
        target_mapped: bool,
    },
    Edit {
        source_tracked: bool,
        policy_allows: bool,
    },
    Delete,
    Reaction {
        target_mapped: bool,
    },
    Topic {
        compatible_target: bool,
    },
    Poll {
        compatible_privacy: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Carry {
    /// Carried with the native semantic.
    Native,
    /// Carried as plain text (formatting dropped).
    PlainText,
    /// Carried as a new message that says it is an edited version.
    NewEditedMessage,
    /// Carried as a message without its reply link.
    UnlinkedMessage,
    /// Carried flattened with a visible context marker.
    Flattened,
    /// Carried as a read-only summary or link, not a poll.
    ReadOnlySummary,
    /// Not carried at all.
    NotCarried,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub carry: Carry,
    /// Sorted, unique, never `exact` together with another flag.
    pub flags: Vec<u8>,
    /// A delete is a request at most; this is always false.
    pub claims_erasure: bool,
}

fn plan(carry: Carry, flags: &[u8]) -> Plan {
    let mut f: Vec<u8> = flags.to_vec();
    f.sort_unstable();
    f.dedup();
    if f.len() > 1 {
        f.retain(|x| *x != EXACT);
    }
    Plan {
        carry,
        flags: f,
        claims_erasure: false,
    }
}

impl Support {
    fn has(&self, s: u8) -> bool {
        self.semantics.contains(&s)
    }

    /// How an operation is carried to the foreign network.
    pub fn translate(&self, op: Op) -> Plan {
        use Carry::*;
        match op {
            Op::Text { rich } => {
                if !self.has(sem::TEXT) {
                    return plan(NotCarried, &[LOSSY_FORMATTING]);
                }
                if rich && !self.has(sem::RICH_TEXT) {
                    plan(PlainText, &[LOSSY_FORMATTING])
                } else {
                    plan(Native, &[EXACT])
                }
            }
            Op::Attachment { bytes } => {
                if !self.has(sem::ATTACHMENTS) {
                    return plan(NotCarried, &[LOSSY_FORMATTING]);
                }
                match self.max_attachment_bytes {
                    Some(max) if bytes > max => plan(Native, &[ATTACHMENT_TRANSCODED]),
                    _ => plan(Native, &[EXACT]),
                }
            }
            Op::Reply { target_mapped } => {
                if self.has(sem::REPLIES) && target_mapped {
                    plan(Native, &[EXACT])
                } else {
                    plan(UnlinkedMessage, &[REPLY_TARGET_UNAVAILABLE])
                }
            }
            Op::Edit {
                source_tracked,
                policy_allows,
            } => {
                if self.has(sem::EDITS) && source_tracked && policy_allows {
                    plan(Native, &[EXACT])
                } else {
                    plan(NewEditedMessage, &[EDIT_AS_NEW_MESSAGE])
                }
            }
            Op::Delete => {
                if self.has(sem::DELETES) {
                    plan(Native, &[EXACT])
                } else {
                    plan(NotCarried, &[DELETE_NOT_PROPAGATED])
                }
            }
            Op::Reaction { target_mapped } => {
                if !self.has(sem::REACTIONS) {
                    plan(NotCarried, &[REACTION_NOT_SUPPORTED])
                } else if !target_mapped {
                    plan(NotCarried, &[REPLY_TARGET_UNAVAILABLE])
                } else {
                    plan(Native, &[EXACT])
                }
            }
            Op::Topic { compatible_target } => {
                if self.has(sem::THREADS_TOPICS) && compatible_target {
                    plan(Native, &[EXACT])
                } else {
                    plan(Flattened, &[LOSSY_FORMATTING])
                }
            }
            Op::Poll { compatible_privacy } => {
                if self.has(sem::POLLS) && compatible_privacy {
                    plan(Native, &[EXACT])
                } else {
                    plan(ReadOnlySummary, &[LOSSY_FORMATTING])
                }
            }
        }
    }
}

/// Adds the provenance flags an inbound event needs and normalizes the set (`exact` stays alone only when nothing else
/// applies).
pub fn with_provenance(mut flags: Vec<u8>, actor_verified: bool, origin_verified: bool) -> Vec<u8> {
    if !actor_verified {
        flags.push(FOREIGN_ACTOR_UNVERIFIED);
    }
    if !origin_verified {
        flags.push(ORIGIN_UNVERIFIED);
    }
    flags.sort_unstable();
    flags.dedup();
    if flags.len() > 1 {
        flags.retain(|x| *x != EXACT);
    }
    flags
}

/// The invariant the wire enforces on a flag set: non-empty, in range, sorted, unique, `exact` alone.
pub fn flags_valid(flags: &[u8]) -> bool {
    !flags.is_empty()
        && flags.iter().all(|f| (1..=9).contains(f))
        && flags.windows(2).all(|w| w[0] < w[1])
        && (!flags.contains(&EXACT) || flags.len() == 1)
}
