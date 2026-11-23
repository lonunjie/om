// SPDX-License-Identifier: MPL-2.0
//! Rehydrate into the same account (portability slice 4; IMPORT_EXPORT_AND_USER_ESCAPE.md section 10 and "Disappearing and
//! ViewOnce content", ADR-0129, ADR-0263). Pure planner over an [`ImportView`] and live state.
//!
//! Allowed only on a device that is already authorized for the archive's account. The plan can repopulate local history
//! and indexes; it has no field for control state, floors, devices, memberships or sessions, so a stale archive cannot
//! lower a floor, resurrect a revoked device or override newer control state. Control objects the archive carries are
//! history, or are listed for live validation by the protocol; they are never applied here. Sessions are always fresh.
//! ViewOnce items are never rehydrated; disappearing items are dropped when expired and scheduled at their original expiry,
//! or skipped entirely when the importer cannot enforce expiry.

use crate::import::{ImportView, LiveControl, ObjectStatus};

/// A history item the archive carries, as the caller's verifier classified it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub view_once: bool,
    /// Canonical absolute expiry for a disappearing item.
    pub expiry: Option<u64>,
    /// The item was authored by this device id, when the archive says so.
    pub device: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The importing client is not an authorized or recovered device for the archive's account.
    NotAuthorizedDevice,
    /// The archive is for another account.
    WrongAccount,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Items that repopulate local history now.
    pub import: Vec<String>,
    /// Disappearing items to delete at their original expiry: `(id, expiry)`.
    pub schedule_deletion: Vec<(String, u64)>,
    pub dropped_expired: Vec<String>,
    pub refused_view_once: Vec<String>,
    /// Disappearing items skipped because this importer cannot enforce expiry.
    pub skipped_unenforceable: Vec<String>,
    /// Items attributed to a device that live state shows revoked: kept as history, never as device authority.
    pub history_only_revoked_device: Vec<String>,
    /// Streams where the archive holds a newer or unknown control version: the protocol must validate it live.
    pub needs_live_validation: Vec<String>,
    /// Control objects older than live state: evidence only.
    pub stale_control: Vec<String>,
    /// Sessions are never restored from an archive.
    pub fresh_sessions_required: bool,
}

pub struct Context<'a> {
    pub authorized_device: bool,
    pub account_matches: bool,
    pub now: u64,
    pub can_enforce_expiry: bool,
    pub live: &'a dyn LiveControl,
    /// Device ids live state shows revoked or quarantined.
    pub revoked_devices: &'a [String],
}

/// Floors only come from live state; an archive cannot raise or lower one.
pub fn floor_after(live: Option<u64>, _archived: u64) -> Option<u64> {
    live
}

pub fn plan(view: &ImportView, items: &[Item], cx: &Context<'_>) -> Result<Plan, Refusal> {
    if !cx.authorized_device {
        return Err(Refusal::NotAuthorizedDevice);
    }
    if !cx.account_matches {
        return Err(Refusal::WrongAccount);
    }
    let mut p = Plan {
        fresh_sessions_required: true,
        ..Default::default()
    };
    for (name, st) in &view.objects {
        match st {
            ObjectStatus::StaleControl { .. } => p.stale_control.push(name.clone()),
            ObjectStatus::ControlNeedsLiveValidation { .. } => {
                p.needs_live_validation.push(name.clone())
            }
            _ => {}
        }
    }
    for it in items {
        if it.view_once {
            p.refused_view_once.push(it.id.clone());
            continue;
        }
        if let Some(exp) = it.expiry {
            if exp <= cx.now {
                p.dropped_expired.push(it.id.clone());
                continue;
            }
            if !cx.can_enforce_expiry {
                p.skipped_unenforceable.push(it.id.clone());
                continue;
            }
            p.schedule_deletion.push((it.id.clone(), exp));
        }
        if it
            .device
            .as_ref()
            .is_some_and(|d| cx.revoked_devices.contains(d))
        {
            p.history_only_revoked_device.push(it.id.clone());
        }
        p.import.push(it.id.clone());
    }
    Ok(p)
}
