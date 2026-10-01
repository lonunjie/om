// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-004. Not product code. Not for production.
//
// Probes whether OpenMLS 0.9.0 supports the application-level GroupCommitOrdering behaviour that
// GROUP_COMMIT_AND_STATE_CONVERGENCE.md §3-§5 and ADR-0163 require. Every scenario reports booleans/counts
// only, so the JSON is identical on every runtime (native, wasm32-wasip1, ...) and across runs.
use openmls::group::{JoinBuilder, ProcessedWelcome};
use openmls::prelude::tls_codec::{Deserialize, Serialize};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use serde_json::{json, Value};

type E = Box<dyn std::error::Error>;
type R<T> = Result<T, E>;

const CS: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

pub struct Dev {
    pub name: &'static str,
    pub provider: OpenMlsRustCrypto,
    pub cred: CredentialWithKey,
    pub signer: SignatureKeyPair,
    pub group: Option<MlsGroup>,
}

impl Dev {
    fn merge_pending(&mut self) -> R<()> {
        self.group.as_mut().ok_or("no group")?.merge_pending_commit(&self.provider)?;
        Ok(())
    }
    fn clear_pending(&mut self) -> R<()> {
        self.group.as_mut().ok_or("no group")?.clear_pending_commit(self.provider.storage())?;
        Ok(())
    }
    fn merge_staged(&mut self, sc: StagedCommit) -> R<()> {
        self.group.as_mut().ok_or("no group")?.merge_staged_commit(&self.provider, sc)?;
        Ok(())
    }
    fn process_raw(&mut self, bytes: &[u8]) -> R<ProcessedMessageContent> {
        let pm = protocol_msg(bytes)?;
        let g = self.group.as_mut().ok_or("no group")?;
        Ok(g.process_message(&self.provider, pm)?.into_content())
    }
    fn app_msg(&mut self, text: &[u8]) -> R<Vec<u8>> {
        let g = self.group.as_mut().ok_or("no group")?;
        bytes_of(&g.create_message(&self.provider, &self.signer, text)?)
    }
}

fn mk_dev(name: &'static str) -> R<Dev> {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(CS.signature_algorithm())?;
    signer.store(provider.storage())?;
    let cred = CredentialWithKey {
        credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
        signature_key: signer.to_public_vec().into(),
    };
    Ok(Dev { name, provider, cred, signer, group: None })
}

fn key_package(d: &Dev) -> R<KeyPackage> {
    Ok(KeyPackage::builder()
        .build(CS, &d.provider, &d.signer, d.cred.clone())?
        .key_package()
        .clone())
}

fn create_config(past_epochs: usize) -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(CS)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(past_epochs)
        .build()
}

fn bytes_of(m: &MlsMessageOut) -> R<Vec<u8>> {
    Ok(m.tls_serialize_detached()?)
}

fn protocol_msg(bytes: &[u8]) -> R<ProtocolMessage> {
    Ok(MlsMessageIn::tls_deserialize_exact(bytes)?.try_into_protocol_message()?)
}

/// Group of `names.len()` devices; the first creates it and adds the rest in one commit.
pub fn make_group(names: &[&'static str], past_epochs: usize) -> R<Vec<Dev>> {
    let mut devs: Vec<Dev> = names.iter().map(|n| mk_dev(n)).collect::<R<_>>()?;
    let cfg = create_config(past_epochs);
    let mut g = MlsGroup::new(&devs[0].provider, &devs[0].signer, &cfg, devs[0].cred.clone())?;
    let kps: Vec<KeyPackage> = devs[1..].iter().map(key_package).collect::<R<_>>()?;
    let (_c, welcome, _gi) = g.add_members(&devs[0].provider, &devs[0].signer, &kps)?;
    g.merge_pending_commit(&devs[0].provider)?;
    let w = welcome.tls_serialize_detached()?;
    devs[0].group = Some(g);
    for d in devs[1..].iter_mut() {
        let wm = match MlsMessageIn::tls_deserialize_exact(&w[..])?.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err("not a welcome".into()),
        };
        let sw = StagedWelcome::new_from_welcome(&d.provider, cfg.join_config(), wm, None)?;
        d.group = Some(sw.into_group(&d.provider)?);
    }
    Ok(devs)
}

fn leaf_of(viewer: &Dev, name: &str) -> Option<LeafNodeIndex> {
    viewer.group.as_ref()?.members().find_map(|m| {
        let bc = BasicCredential::try_from(m.credential.clone()).ok()?;
        (bc.identity() == name.as_bytes()).then_some(m.index)
    })
}

fn member_names(d: &Dev) -> Vec<String> {
    let mut v: Vec<String> = d
        .group
        .as_ref()
        .map(|g| {
            g.members()
                .filter_map(|m| BasicCredential::try_from(m.credential).ok())
                .map(|b| String::from_utf8_lossy(b.identity()).to_string())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn exporter(d: &Dev) -> Option<Vec<u8>> {
    d.group.as_ref()?.export_secret(d.provider.crypto(), "om-v004", &[], 32).ok()
}

fn epoch(d: &Dev) -> u64 {
    d.group.as_ref().map(|g| g.epoch().as_u64()).unwrap_or(u64::MAX)
}

/// Build (and stage, but do NOT merge) a direct reducing commit. Returns the exact serialized commit.
fn prepare_remove(d: &mut Dev, target: &str) -> R<Vec<u8>> {
    let t = leaf_of(d, target).ok_or("target not a member")?;
    let g = d.group.as_mut().ok_or("no group")?;
    let b = g
        .commit_builder()
        .propose_removals([t])
        .load_psks(d.provider.storage())?
        .build(d.provider.rand(), d.provider.crypto(), &d.signer, |_| true)?
        .stage_commit(&d.provider)?;
    bytes_of(b.commit())
}

fn prepare_self_update(d: &mut Dev) -> R<Vec<u8>> {
    let g = d.group.as_mut().ok_or("no group")?;
    let b = g
        .commit_builder()
        .force_self_update(true)
        .load_psks(d.provider.storage())?
        .build(d.provider.rand(), d.provider.crypto(), &d.signer, |_| true)?
        .stage_commit(&d.provider)?;
    bytes_of(b.commit())
}

/// Receiver side: stage a commit without merging. Returns the staged commit for policy inspection.
fn stage(d: &mut Dev, bytes: &[u8]) -> R<StagedCommit> {
    let pm = protocol_msg(bytes)?;
    let g = d.group.as_mut().ok_or("no group")?;
    match g.process_message(&d.provider, pm)?.into_content() {
        ProcessedMessageContent::StagedCommitMessage(sc) => Ok(*sc),
        _ => Err("not a commit".into()),
    }
}

fn accept(d: &mut Dev, bytes: &[u8]) -> R<()> {
    let sc = stage(d, bytes)?;
    d.merge_staged(sc)?;
    Ok(())
}

fn converged(devs: &[&Dev]) -> bool {
    let first = devs[0];
    devs.iter().all(|d| {
        epoch(d) == epoch(first)
            && exporter(d).is_some()
            && exporter(d) == exporter(first)
            && member_names(d) == member_names(first)
    })
}

// ------------------------------------------------------------------------------------------------
// S1. Application-ordered same-epoch race, committers hold their commits un-merged.
//     `winner_is_a`: whose commit the application ordering profile picks.
// ------------------------------------------------------------------------------------------------
pub fn s1_ordered_race(winner_a: bool) -> R<Value> {
    // A removes D; C removes B. Same parent epoch.
    let mut v = make_group(&["A", "B", "C", "D"], 2)?;
    let e0 = epoch(&v[0]);
    let ca = prepare_remove(&mut v[0], "D")?;
    let cc = prepare_remove(&mut v[2], "B")?;
    let both_pending = v[0].group.as_ref().unwrap().pending_commit().is_some()
        && v[2].group.as_ref().unwrap().pending_commit().is_some();
    let (win, win_idx, lose_idx, lose_target) = if winner_a { (&ca, 0, 2, "B") } else { (&cc, 2, 0, "D") };
    let win = win.clone();
    let lose_bytes = if winner_a { cc.clone() } else { ca.clone() };
    // Winner's author merges; loser's author discards its pending commit; everyone else stages+merges.
    v[win_idx].merge_pending()?;
    v[lose_idx].clear_pending()?;
    let mut removed_noticed = false;
    for i in 0..4 {
        if i == win_idx {
            continue;
        }
        let sc = stage(&mut v[i], &win)?;
        let removed = sc.self_removed();
        v[i].merge_staged(sc)?;
        if removed {
            removed_noticed = !v[i].group.as_ref().unwrap().is_active();
        }
    }
    // Everyone still in the group is at e0+1 with identical secrets; the removed device is inactive.
    let removed_name = if winner_a { "D" } else { "B" };
    let live: Vec<&Dev> = v.iter().filter(|d| d.name != removed_name).collect();
    let conv1 = converged(&live) && epoch(live[0]) == e0 + 1;
    // The loser commit is dead at the new epoch for every receiver.
    let stale_rejected = {
        let tgt = v.iter_mut().find(|d| d.name != removed_name && d.name != if winner_a { "C" } else { "A" }).unwrap();
        stage(tgt, &lose_bytes).is_err()
    };
    // Rebase: the loser's author re-issues its still-applicable reduction at the new parent.
    let lname = if winner_a { "C" } else { "A" };
    let li = v.iter().position(|d| d.name == lname).unwrap();
    let rebase = prepare_remove(&mut v[li], lose_target)?;
    v[li].merge_pending()?;
    for i in 0..4 {
        if i == li || v[i].name == removed_name {
            continue;
        }
        accept(&mut v[i], &rebase)?;
    }
    let final_live: Vec<&Dev> = v.iter().filter(|d| d.name != "B" && d.name != "D").collect();
    let conv2 = converged(&final_live) && epoch(final_live[0]) == e0 + 2;
    let final_members = member_names(final_live[0]);
    Ok(json!({
        "both_candidates_held_pending_unmerged": both_pending,
        "winner_converged_epoch_plus_1": conv1,
        "removed_device_inactive_after_winner": removed_noticed,
        "loser_commit_rejected_at_new_epoch": stale_rejected,
        "rebased_reduction_converged_epoch_plus_2": conv2,
        "final_roster_is_A_C": final_members == vec!["A", "C"],
    }))
}

// ------------------------------------------------------------------------------------------------
// S1b. Mutual removal: A removes C while C removes A. Whichever wins, the loser is out and its
//      pending commit is not usable; nobody is left in two branches.
// ------------------------------------------------------------------------------------------------
pub fn s1b_mutual_removal(winner_a: bool) -> R<Value> {
    let mut v = make_group(&["A", "B", "C"], 2)?;
    let ca = prepare_remove(&mut v[0], "C")?;
    let cc = prepare_remove(&mut v[2], "A")?;
    let (win, wi, li) = if winner_a { (ca, 0, 2) } else { (cc, 2, 0) };
    v[wi].merge_pending()?;
    // The loser's author is the removed device of the winner: it processes the winner and goes inactive.
    v[li].clear_pending()?;
    let sc = stage(&mut v[li], &win)?;
    let self_removed = sc.self_removed();
    v[li].merge_staged(sc)?;
    accept(&mut v[1], &win)?;
    let inactive = !v[li].group.as_ref().unwrap().is_active();
    let survivors: Vec<&Dev> = vec![&v[wi], &v[1]];
    Ok(json!({
        "loser_author_learns_it_was_removed": self_removed,
        "loser_author_inactive": inactive,
        "survivors_converged": converged(&survivors),
        "survivor_roster_size": member_names(&v[1]).len(),
    }))
}

// ------------------------------------------------------------------------------------------------
// S2. Both partitions merge their own direct reducing commit (a direct commit was accepted locally).
//     MLS cannot cross-process the other branch: a real fork. Recovery uses fork-resolution (readd).
// ------------------------------------------------------------------------------------------------
pub fn s2_partition_fork_and_readd() -> R<Value> {
    // Partition 1 = {A, B}; partition 2 = {C, D}. A removes D; C removes B.
    let mut v = make_group(&["A", "B", "C", "D"], 2)?;
    let e0 = epoch(&v[0]);
    let ca = prepare_remove(&mut v[0], "D")?;
    v[0].merge_pending()?;
    accept(&mut v[1], &ca)?; // B in partition 1
    let cc = prepare_remove(&mut v[2], "B")?;
    v[2].merge_pending()?;
    accept(&mut v[3], &cc)?; // D in partition 2 (D is removed only on branch 1)
    // Partition 2 keeps working on its branch: C sends an application message under the loser branch.
    let loser_msg = v[2].app_msg(b"sent on loser branch")?;
    // Branches have diverged: same epoch number, different state.
    let same_epoch = epoch(&v[0]) == epoch(&v[2]) && epoch(&v[0]) == e0 + 1;
    let control_not_converged = !converged(&[&v[0], &v[2]]);
    let tags_differ = v[0].group.as_ref().unwrap().confirmation_tag() != v[2].group.as_ref().unwrap().confirmation_tag();
    // Cross-processing is impossible in both directions.
    let a_reject_c = stage(&mut v[0], &cc).is_err();
    let c_reject_a = stage(&mut v[2], &ca).is_err();
    // The winner branch cannot read the loser branch's application message.
    let winner_cannot_read_loser_msg = v[0].process_raw(&loser_msg).is_err();

    // Resolution. Winner = partition 1 (A's commit). A re-adds the complement of its own partition.
    let c_new_kp = key_package(&v[2])?;
    let own = [v[0].group.as_ref().unwrap().own_leaf_index(), v[1].group.as_ref().unwrap().own_leaf_index()];
    let (complement, readd) = {
        let Dev { group, provider, signer, .. } = &mut v[0];
        let builder = group.as_mut().unwrap().recover_fork_by_readding(&own)?;
        let complement: Vec<String> = builder
            .complement_partition()
            .iter()
            .map(|m| String::from_utf8_lossy(BasicCredential::try_from(m.credential.clone()).unwrap().identity()).to_string())
            .collect();
        let readd = builder
            .provide_key_packages(vec![c_new_kp])
            .load_psks(provider.storage())?
            .build(provider.rand(), provider.crypto(), &*signer, |_| true)?
            .stage_commit(&*provider)?;
        (complement, readd)
    };
    let (commit, welcome, _) = readd.into_contents();
    let commit_b = bytes_of(&commit)?;
    let welcome = welcome.ok_or("no welcome")?;
    let cfg = create_config(2);
    let pw = ProcessedWelcome::new_from_welcome(&v[2].provider, cfg.join_config(), welcome)?;
    let new_c = JoinBuilder::new(&v[2].provider, pw).replace_old_group().build()?.into_group(&v[2].provider)?;
    v[0].merge_pending()?;
    accept(&mut v[1], &commit_b)?;
    v[2].group = Some(new_c);
    // D: removed on the canonical branch; it holds only the loser branch and cannot process the winner.
    let d_cannot_process_winner = stage(&mut v[3], &commit_b).is_err();
    // After readd, A, B, C agree. D is not part of it.
    let conv_after_readd = converged(&[&v[0], &v[1], &v[2]]);
    // C's still-applicable reduction (remove B) is rebased as a fresh commit on the winner branch.
    let rebase = prepare_remove(&mut v[2], "B")?;
    v[2].merge_pending()?;
    accept(&mut v[0], &rebase)?;
    let conv_final = converged(&[&v[0], &v[2]]);
    let roster = member_names(&v[0]);
    Ok(json!({
        "branches_same_epoch_number": same_epoch,
        "branches_have_different_state": tags_differ,
        "control_converged_check_is_false_on_diverged_branches": control_not_converged,
        "partition1_rejects_partition2_commit": a_reject_c,
        "partition2_rejects_partition1_commit": c_reject_a,
        "winner_branch_cannot_read_loser_branch_message": winner_cannot_read_loser_msg,
        "readd_complement_is_exactly_C": complement == vec!["C"],
        "removed_device_cannot_process_winner_commit": d_cannot_process_winner,
        "converged_after_readd": conv_after_readd,
        "converged_after_rebased_reduction": conv_final,
        "final_roster_is_A_C": roster == vec!["A", "C"],
    }))
}

// ------------------------------------------------------------------------------------------------
// S3. External-commit rejoin and the application membership gate.
// ------------------------------------------------------------------------------------------------
pub fn s3_external_commit_policy() -> R<Value> {
    let mut v = make_group(&["A", "B", "C"], 2)?;
    // C is removed by A and B accepts.
    let rm = prepare_remove(&mut v[0], "C")?;
    v[0].merge_pending()?;
    accept(&mut v[1], &rm)?;
    let epoch_before = epoch(&v[0]);
    // The removed device C builds an external commit from the current public GroupInfo.
    let gi = v[0].group.as_ref().unwrap().export_group_info(v[0].provider.crypto(), &v[0].signer, true)?;
    let gi_bytes = bytes_of(&gi)?;
    let vgi = match MlsMessageIn::tls_deserialize_exact(&gi_bytes[..])?.extract() {
        MlsMessageBodyIn::GroupInfo(g) => g,
        _ => return Err("not group info".into()),
    };
    let cfg = create_config(2);
    let (_cg, bundle) = MlsGroup::external_commit_builder()
        .with_config(cfg.join_config().clone())
        .build_group(&v[2].provider, vgi, v[2].cred.clone())?
        .load_psks(v[2].provider.storage())?
        .build(v[2].provider.rand(), v[2].provider.crypto(), &v[2].signer, |_| true)?
        .finalize(&v[2].provider)?;
    let (ext_commit, _, _) = bundle.into_contents();
    let ext_bytes = bytes_of(&ext_commit)?;
    // MLS itself accepts the staged external commit; the OM roster check must reject it before merge.
    let sc = stage(&mut v[0], &ext_bytes)?;
    let creds: Vec<String> = sc
        .credentials_to_verify()
        .filter_map(|c| BasicCredential::try_from(c.clone()).ok())
        .map(|b| String::from_utf8_lossy(b.identity()).to_string())
        .collect();
    let roster = ["A", "B"]; // canonical GroupControlState roster after the accepted removal
    let blocked = creds.iter().any(|c| !roster.contains(&c.as_str()));
    drop(sc); // not merged: the staged commit is discarded
    let state_unchanged = epoch(&v[0]) == epoch_before && member_names(&v[0]) == vec!["A", "B"];
    // The group keeps working: B can still commit and A still accepts it.
    let up = prepare_self_update(&mut v[1])?;
    v[1].merge_pending()?;
    accept(&mut v[0], &up)?;
    Ok(json!({
        "library_stages_external_commit_from_removed_device": true,
        "staged_commit_exposes_joiner_credential_for_roster_check": creds == vec!["C"],
        "roster_check_would_block_it": blocked,
        "discarded_staged_commit_leaves_group_unchanged": state_unchanged,
        "group_still_advances_after_discard": converged(&[&v[0], &v[1]]),
    }))
}

// ------------------------------------------------------------------------------------------------
// S4. ADR-0163 crash/restart: the prepared commit survives a restart; exact retry is idempotent.
// ------------------------------------------------------------------------------------------------
pub fn s4_crash_restart() -> R<Value> {
    let mut v = make_group(&["A", "B", "C"], 2)?;
    let gid = v[0].group.as_ref().unwrap().group_id().clone();
    let prepared = prepare_remove(&mut v[0], "C")?; // CANDIDATE_PREPARED
    // "Crash": a new provider with a copy of the persisted storage; the in-memory group is dropped.
    let p2 = OpenMlsRustCrypto::default();
    {
        let src = v[0].provider.storage().values.read().unwrap();
        *p2.storage().values.write().unwrap() = src.clone();
    }
    let mut restored = MlsGroup::load(p2.storage(), &gid)?.ok_or("group not restored")?;
    let pending_survives = restored.pending_commit().is_some();
    // PUBLISHED_UNCONFIRMED: the exact stored bytes go out; peers stage and accept them.
    accept(&mut v[1], &prepared)?;
    // Exact retry: a duplicate delivery is rejected by MLS (stale epoch), so the app-level
    // accepted-commit/dedupe marker is the only thing that has to make a duplicate idempotent.
    let dup_rejected = stage(&mut v[1], &prepared).is_err();
    // ACCEPTED: the restored node promotes the same commit.
    restored.merge_pending_commit(&p2)?;
    let restored_epoch_ok = restored.epoch() == v[1].group.as_ref().unwrap().epoch();
    let same_secret = restored.export_secret(p2.crypto(), "om-v004", &[], 32)? == exporter(&v[1]).unwrap();
    // Candidate whose pending state was lost: no pending commit after a restore without it.
    let p3 = OpenMlsRustCrypto::default();
    let mut v2 = make_group(&["A", "B"], 2)?;
    let gid2 = v2[0].group.as_ref().unwrap().group_id().clone();
    let _ = prepare_remove(&mut v2[0], "B")?;
    {
        let src = v2[0].provider.storage().values.read().unwrap();
        *p3.storage().values.write().unwrap() = src.clone();
    }
    let mut lost = MlsGroup::load(p3.storage(), &gid2)?.ok_or("group not restored")?;
    lost.clear_pending_commit(p3.storage())?;
    let cleared = lost.pending_commit().is_none();
    Ok(json!({
        "prepared_commit_persisted_and_restored": pending_survives,
        "restored_node_promotes_same_commit_and_converges": restored_epoch_ok && same_secret,
        "duplicate_delivery_rejected_by_mls_at_new_epoch": dup_rejected,
        "pending_commit_can_be_discarded_after_restore": cleared,
    }))
}

// ------------------------------------------------------------------------------------------------
// S5. past_epoch_window (ADR-0121 rule 4): messages from older epochs decrypt only inside the window.
// ------------------------------------------------------------------------------------------------
pub fn s5_past_epoch_window(window: usize) -> R<Value> {
    let mut v = make_group(&["A", "B", "C"], window)?;
    // A encrypts one message per future lag at epoch e0.
    let msgs: Vec<Vec<u8>> = (0..5)
        .map(|i| v[0].app_msg(format!("m{i}").as_bytes()))
        .collect::<R<_>>()?;
    let mut ok_by_lag = Vec::new();
    for lag in 1..=4usize {
        let up = prepare_self_update(&mut v[2])?;
        v[2].merge_pending()?;
        accept(&mut v[0], &up)?;
        accept(&mut v[1], &up)?;
        let ok = matches!(v[1].process_raw(&msgs[lag]), Ok(ProcessedMessageContent::ApplicationMessage(_)));
        ok_by_lag.push(ok);
    }
    let in_window_ok = ok_by_lag.iter().take(window).all(|b| *b);
    let out_window_fails = ok_by_lag.iter().skip(window).all(|b| !*b);
    Ok(json!({
        "window": window,
        "decrypts_by_lag_1_to_4": ok_by_lag,
        "inside_window_decrypts": in_window_ok,
        "outside_window_fails": out_window_fails,
    }))
}

pub fn run_all() -> R<Value> {
    let s1a = s1_ordered_race(true)?;
    let s1b = s1_ordered_race(false)?;
    let s1ba = s1b_mutual_removal(true)?;
    let s1bb = s1b_mutual_removal(false)?;
    let s2 = s2_partition_fork_and_readd()?;
    let s3 = s3_external_commit_policy()?;
    let s4 = s4_crash_restart()?;
    let s5 = s5_past_epoch_window(2)?;
    Ok(json!({
        "library": "openmls 0.9.0 (rust_crypto provider, memory storage, fork-resolution feature)",
        "ciphersuite": "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
        "s1_ordered_same_epoch_race": {"winner_A": s1a, "winner_C": s1b},
        "s1b_mutual_removal": {"winner_A": s1ba, "winner_C": s1bb},
        "s2_partition_fork_and_readd": s2,
        "s3_external_commit_policy": s3,
        "s4_crash_restart": s4,
        "s5_past_epoch_window": s5,
    }))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    pub fn run_all_json() -> Result<String, JsError> {
        let v = super::run_all().map_err(|e| JsError::new(&e.to_string()))?;
        Ok(serde_json::to_string_pretty(&v).unwrap())
    }
}
