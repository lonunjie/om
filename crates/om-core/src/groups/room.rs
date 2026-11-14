// SPDX-License-Identifier: MPL-2.0
//! Room protection profiles and what a change of them means (ADR-0189 section 3).
//!
//! The profile names are the existing ones (Recommended, Hardened, Maximum Privacy); `Custom` is never a stored value,
//! it is what [`custom`] reports when a Room's fields differ from the bundle of its profile. A profile sets Room fields
//! at creation only; nothing here runs on an update, a theme or a new default, so an existing Room never changes by
//! itself. A change that makes a Room less strict is classified by [`diff`], which is what the flow uses to demand an
//! explicit confirmation and to announce the change to every member. The cryptographic floor is not touched by any
//! profile (SECURITY_PRIVACY_PROFILES section 1).

use super::{GroupError, GroupState, R, RoleDef, err};
use serde::{Deserialize, Serialize};

pub const RECOMMENDED: u64 = 1;
pub const HARDENED: u64 = 2;
pub const MAXIMUM_PRIVACY: u64 = 3;

/// The Room fields a protection profile sets (ADR-0189 section 3, "Per Room"). Disappearing messages are off in every
/// profile (opt-in), and invite links are off until an admin turns them on, so neither is a field here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bundle {
    /// 1 `invite_only` (an admission holder's device completes the add by itself), 2 `admin_approval`.
    pub join_rule: u64,
    /// 1 `automatic`, 2 `admin_approval`.
    pub new_device_policy: u64,
    /// 1 `since_join`: new people do not see earlier messages, in every profile.
    pub history_policy: u64,
    pub security_profile: u64,
    /// Whether the `Member` role holds `room.invite` ("Anyone in the group can add people").
    pub members_may_invite: bool,
}

/// The bundle of `profile`. Values 4 to 65535 are reserved: refused, never guessed.
pub fn bundle(profile: u64) -> R<Bundle> {
    Ok(match profile {
        RECOMMENDED => Bundle {
            join_rule: 1,
            new_device_policy: 1,
            history_policy: 1,
            security_profile: RECOMMENDED,
            members_may_invite: true,
        },
        HARDENED => Bundle {
            join_rule: 2,
            new_device_policy: 1,
            history_policy: 1,
            security_profile: HARDENED,
            members_may_invite: false,
        },
        MAXIMUM_PRIVACY => Bundle {
            join_rule: 2,
            new_device_policy: 2,
            history_policy: 1,
            security_profile: MAXIMUM_PRIVACY,
            members_may_invite: false,
        },
        _ => return err("security_profile_reserved"),
    })
}

/// One Room setting a person can change (the typed intent of the Room settings screen).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    /// Move every field to the bundle of this profile (one Commit per field that differs).
    Protection(u64),
    JoinRule(u64),
    NewDevicePolicy(u64),
    HistoryPolicy(u64),
    MembersMayInvite(bool),
    /// Seconds; `None` turns it off.
    DisappearingTimer(Option<u64>),
}

/// The Room's `Member` role, by its display name (as the flow creates it).
pub fn member_role(g: &GroupState) -> Option<&RoleDef> {
    g.role_definitions
        .iter()
        .find(|r| r.display_name == "Member")
}

/// Whether anyone in the group can add people (the `Member` role holds `room.invite`).
pub fn members_may_invite(g: &GroupState) -> bool {
    member_role(g).is_some_and(|r| r.grants.iter().any(|c| c == "room.invite"))
}

/// Whether the Room's fields differ from the bundle of its own protection level (the client shows "Custom").
pub fn custom(g: &GroupState) -> bool {
    match bundle(g.policy.security_profile) {
        Ok(b) => {
            b.join_rule != g.policy.join_rule
                || b.new_device_policy != g.policy.new_device_policy
                || b.history_policy != g.policy.history_policy
                || b.members_may_invite != members_may_invite(g)
        }
        Err(_) => true,
    }
}

/// One visible difference between two states of a Room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// A stable code: `join_rule`, `new_device_policy`, `history_policy`, `members_may_invite`, `disappearing_timer`,
    /// `security_profile`.
    pub field: String,
    pub from: String,
    pub to: String,
    /// The Room became less strict: subject to explicit confirmation and always announced (ADR-0189 section 3).
    pub loosened: bool,
}

/// The plain-language title of a field, as ADR-0189 section 3 words it (the client may translate).
pub fn label(field: &str) -> &'static str {
    match field {
        "members_may_invite" => "Who can add people",
        "join_rule" => "Do new people need an admin's OK?",
        "history_policy" => "Earlier messages for new people",
        "disappearing_timer" => "Disappearing messages",
        "new_device_policy" => "A member's new device",
        "security_profile" => "Protection level",
        _ => "Room setting",
    }
}

fn join_rule(v: u64) -> (&'static str, u8) {
    match v {
        1 => ("invite_only", 1),
        2 => ("admin_approval", 2),
        _ => ("open", 0),
    }
}
fn new_device(v: u64) -> (&'static str, u8) {
    match v {
        2 => ("admin_approval", 1),
        _ => ("automatic", 0),
    }
}
fn history(v: u64) -> (&'static str, u8) {
    match v {
        1 => ("since_join", 2),
        2 => ("recent_window", 1),
        _ => ("full_shared_history", 0),
    }
}
fn profile(v: u64) -> (&'static str, u8) {
    match v {
        1 => ("recommended", 1),
        2 => ("hardened", 2),
        3 => ("maximum_privacy", 3),
        _ => ("reserved", 0),
    }
}

/// Every setting that differs between `before` and `after`, each marked loosened when `after` is less strict.
pub fn diff(before: &GroupState, after: &GroupState) -> Vec<Change> {
    let mut out = vec![];
    let mut ch = |field: &str, from: String, to: String, loosened: bool| {
        out.push(Change {
            field: field.into(),
            from,
            to,
            loosened,
        })
    };
    let (b, a) = (&before.policy, &after.policy);
    if b.join_rule != a.join_rule {
        let (f, t) = (join_rule(b.join_rule), join_rule(a.join_rule));
        ch("join_rule", f.0.into(), t.0.into(), t.1 < f.1);
    }
    if b.new_device_policy != a.new_device_policy {
        let (f, t) = (
            new_device(b.new_device_policy),
            new_device(a.new_device_policy),
        );
        ch("new_device_policy", f.0.into(), t.0.into(), t.1 < f.1);
    }
    if b.history_policy != a.history_policy {
        let (f, t) = (history(b.history_policy), history(a.history_policy));
        ch("history_policy", f.0.into(), t.0.into(), t.1 < f.1);
    }
    if b.security_profile != a.security_profile {
        let (f, t) = (profile(b.security_profile), profile(a.security_profile));
        ch("security_profile", f.0.into(), t.0.into(), t.1 < f.1);
    }
    let (mb, ma) = (members_may_invite(before), members_may_invite(after));
    if mb != ma {
        let w = |x: bool| if x { "anyone" } else { "admins_only" };
        ch("members_may_invite", w(mb).into(), w(ma).into(), ma && !mb);
    }
    if b.disappearing_timer != a.disappearing_timer {
        let w = |x: Option<u64>| x.map_or("off".to_string(), |n| n.to_string());
        let loosened = match (b.disappearing_timer, a.disappearing_timer) {
            (Some(_), None) => true,
            (Some(x), Some(y)) => y > x,
            _ => false,
        };
        ch(
            "disappearing_timer",
            w(b.disappearing_timer),
            w(a.disappearing_timer),
            loosened,
        );
    }
    out
}

/// A change refused until the person explicitly confirms it (the typed refusal of an unconfirmed loosening).
pub fn needs_confirmation() -> GroupError {
    GroupError("loosening_needs_confirmation")
}

/// One announced change of a Room, kept so it is never silent even when nobody is looking (a Room notice).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub at_ms: u64,
    /// The account that made the change (hex).
    pub by_account: String,
    pub changes: Vec<Change>,
}
