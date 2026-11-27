// SPDX-License-Identifier: MPL-2.0
//! Engineering-only, profile-scoped observations for ADR-0273.

use om_core::api::ProfileExecutionContext;
use om_core::community_directory::{self as directory, Directory};
use om_core::crypto::cbor::{self, V};
use om_core::crypto::{PROFILE_V1, hex, sha256, sig, unhex};
use om_core::publication::{self, Event, EventOutcome, Feed};
use om_core::room_control::{ControllerAccount, KIND_ACCOUNT, controller_device};
use om_core::space_control::{self, SpaceChain};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_HANDLES: usize = 64;
const MAX_CONTROL_STATES: usize = 64;
const MAX_EVENT_OBSERVATIONS: usize = 512;
const MAX_EVENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const PUBLICATION_CONTROL: &str = "OM-PUBLICATION-CONTROL-SIG-v1";
const PUBLICATION_EVENT: &str = "OM-PUBLICATION-EVENT-SIG-v1";
const SPACE_CONTROL: &str = "OM-SPACE-CONTROL-SIG-v1";
const ROLE_PUBLICATION: u64 = 7;

type Key = (String, [u8; 32]);
type Principal = (u64, Vec<u8>);

#[derive(Default)]
pub struct Handles {
    publications: BTreeMap<Key, Publication>,
    spaces: BTreeMap<Key, Space>,
    directories: BTreeMap<String, Directory>,
}

struct Publication {
    feed: Feed,
    controls: usize,
    events: usize,
    event_bytes: usize,
    held: BTreeMap<[u8; 32], DisplayEvent>,
    posts: BTreeMap<Vec<u8>, DisplayPost>,
}

struct Space {
    chain: SpaceChain,
    controls: usize,
}

struct DisplayEvent {
    time: u64,
    content: Option<String>,
}

struct DisplayPost {
    published_at_ms: u64,
    retracted: bool,
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PublicationOp {
    Open,
    Close,
    AcceptControl { signed: String },
    AcceptEvent { signed: String, public_key: String },
    View,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SpaceOp {
    Open,
    Close,
    AcceptState {
        signed: String,
    },
    /// An optional, caller-observed reachability snapshot, used only to label governance.
    View {
        live_accounts: Option<Vec<String>>,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    Community,
    Room,
    Publication,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferUses {
    OneTime,
    Limited,
    Reusable,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DirectoryOp {
    Observe {
        place: PlaceKind,
        id: String,
        name: String,
        description: Option<String>,
    },
    ObserveOffer {
        place: PlaceKind,
        id: String,
        incarnation: String,
        approval_required: bool,
        uses: Option<OfferUses>,
        expires_at_ms: Option<u64>,
    },
    WithdrawOffer {
        place: PlaceKind,
        id: String,
        target: String,
    },
    Withdraw {
        place: PlaceKind,
        id: String,
    },
    List,
}

fn fixed_hex<const N: usize>(input: &str) -> Option<[u8; N]> {
    if input.len() != N * 2 {
        return None;
    }
    unhex(input).ok()?.try_into().ok()
}

fn refused(reason: &'static str) -> Value {
    json!({ "refused": reason })
}

fn done() -> Value {
    json!({ "done": true })
}

fn outcome(name: &'static str) -> Value {
    json!({ "outcome": name })
}

fn envelope(encoded: &str, max_bytes: usize) -> Result<(Vec<u8>, Vec<V>), &'static str> {
    if encoded.len() > max_bytes * 2 {
        return Err("malformed");
    }
    let invalid = |_| "malformed";
    let wire = unhex(encoded).map_err(invalid)?;
    let V::M(mut fields) = cbor::decode(&wire).map_err(invalid)? else {
        return Err("malformed");
    };
    let (Some((2, V::A(entries))), Some((1, V::B(body)))) = (fields.pop(), fields.pop()) else {
        return Err("malformed");
    };
    if !fields.is_empty() || !(1..=16).contains(&entries.len()) {
        return Err("malformed");
    }
    Ok((body, entries))
}

fn signature(entry: &V, role: u64) -> Result<(&[u8], &[u8]), &'static str> {
    let invalid = |_| "malformed";
    if !matches!(entry, V::M(fields) if fields.len() == 4)
        || entry.get(1).and_then(V::u).map_err(invalid)? != PROFILE_V1
        || entry.get(2).and_then(V::u).map_err(invalid)? != role
    {
        return Err("malformed");
    }
    let signer = entry.get(0).and_then(V::b).map_err(invalid)?;
    let signature = entry.get(3).and_then(V::b).map_err(invalid)?;
    if !(16..=32).contains(&signer.len()) || signature.len() != sig::SIG_LEN {
        return Err("malformed");
    }
    Ok((signer, signature))
}

fn control_approvers(
    accounts: &[ControllerAccount],
    label: &str,
    body: &[u8],
    entries: &[V],
) -> Result<Vec<Principal>, &'static str> {
    let mut devices = BTreeSet::new();
    let mut approvers = BTreeSet::new();
    for entry in entries {
        let (signer, signature) = signature(entry, sig::ROLE_CONTROLLER)?;
        let device = signer.try_into().map_err(|_| "malformed")?;
        if !devices.insert(device) {
            return Err("duplicate_signer");
        }
        let (account, credential) = controller_device(accounts, &device).ok_or("unknown_signer")?;
        let input = sig::ds_sig(label, PROFILE_V1, sig::ROLE_CONTROLLER, signer, body);
        sig::verify(&credential.signing_pk, &input, signature).map_err(|_| "bad_signature")?;
        approvers.insert((KIND_ACCOUNT, account.head.account_id.to_vec()));
    }
    Ok(approvers.into_iter().collect())
}

fn publication_control(
    publication: &mut Publication,
    accounts: &[ControllerAccount],
    signed: &str,
) -> Result<&'static str, &'static str> {
    let (body, entries) = envelope(signed, 65536)?;
    let state = publication::ControlState::from_body(&body).map_err(|_| "malformed")?;
    let approvers = control_approvers(accounts, PUBLICATION_CONTROL, &body, &entries)?;
    // Feed's genesis reducer assumes its authorization has already been verified.
    if state.prev.is_none()
        && (state.version != 1
            || (approvers
                .iter()
                .filter(|principal| state.governance.contains(principal))
                .count() as u64)
                < state.threshold)
    {
        return Err("governance");
    }
    if publication.controls >= MAX_CONTROL_STATES {
        return Err("quota");
    }
    use publication::{ControlError as E, ControlOutcome as O};
    match publication.feed.accept_control(state, &approvers) {
        Ok(O::Duplicate) => Ok("duplicate"),
        Ok(result) => {
            publication.controls += 1;
            Ok(if result == O::Forked {
                "forked"
            } else {
                "accepted"
            })
        }
        Err(error) => Err(match error {
            E::WrongPublication => "wrong_publication",
            E::MissingPrev => "missing_prev",
            E::UnknownPrev => "unknown_prev",
            E::Governance => "governance",
            E::Rollback => "rollback",
            E::Frozen => "frozen",
        }),
    }
}

fn display_event(body: &[u8], kind: publication::Kind) -> Result<DisplayEvent, &'static str> {
    let invalid = |_| "malformed";
    let body = cbor::decode(body).map_err(invalid)?;
    let time = body
        .get(4)
        .and_then(|clock| clock.get(0))
        .and_then(V::u)
        .map_err(invalid)?;
    let payload = body.get(6).map_err(invalid)?;
    let content = match kind {
        publication::Kind::Tombstone => payload
            .opt(0)
            .map(|text| text.t().map(|s| om_core::bridge_room::clean_text(s, 256)))
            .transpose()
            .map_err(invalid)?,
        _ => Some(
            om_core::events::plain_text(payload.get(0).map_err(invalid)?)
                .ok_or("unsupported_content")?,
        ),
    };
    Ok(DisplayEvent {
        time: time.min(MAX_SAFE_INTEGER),
        content,
    })
}

fn publication_event(
    publication: &mut Publication,
    signed: &str,
    public_key: &str,
) -> Result<&'static str, &'static str> {
    let (body, entries) = envelope(signed, 65536)?;
    let event = Event::from_body(&body).map_err(|_| "malformed")?;
    let [entry] = entries.as_slice() else {
        return Err("malformed");
    };
    let (signer, signature) = signature(entry, ROLE_PUBLICATION)?;
    // Engineering supports self-certifying SHA-256 references only. The signed control state
    // commits to the reference; a supplied public key can only prove it knows the preimage.
    let key = fixed_hex::<{ sig::PK_LEN }>(public_key).ok_or("unverifiable_key")?;
    if event.signing_key_ref.as_slice() != signer || signer != sha256(&[&key]).as_slice() {
        return Err("unverifiable_key");
    }
    let input = sig::ds_sig(
        PUBLICATION_EVENT,
        PROFILE_V1,
        ROLE_PUBLICATION,
        signer,
        &body,
    );
    sig::verify(&key, &input, signature).map_err(|_| "bad_signature")?;
    if publication.events >= MAX_EVENT_OBSERVATIONS
        || publication.event_bytes + body.len() > MAX_EVENT_BYTES
    {
        return Err("quota");
    }
    let display = display_event(&body, event.kind)?;
    let hash = event.hash;
    let held = &mut publication.held;
    let posts = &mut publication.posts;
    held.entry(hash).or_insert(display);
    let result = publication.feed.accept_event_with(event, &mut |applied| {
        if let Some(display) = held.remove(&applied.hash) {
            let post = posts.entry(applied.post_id.clone()).or_insert(DisplayPost {
                published_at_ms: display.time,
                retracted: false,
                content: None,
            });
            if !post.retracted {
                post.retracted = applied.kind == publication::Kind::Tombstone;
                post.content = display.content;
            }
        }
    });
    if !matches!(result, Ok(EventOutcome::Held)) {
        held.remove(&hash);
    }
    if matches!(
        result,
        Ok(EventOutcome::Accepted | EventOutcome::Held | EventOutcome::Equivocation)
    ) {
        publication.events += 1;
        publication.event_bytes += body.len();
    }
    use publication::EventError as E;
    match result {
        Ok(EventOutcome::Accepted) => Ok("accepted"),
        Ok(EventOutcome::Duplicate) => Ok("duplicate"),
        Ok(EventOutcome::Equivocation) => Ok("equivocation"),
        Ok(EventOutcome::Held) => Ok("held"),
        Err(error) => Err(match error {
            E::WrongPublication => "wrong_publication",
            E::UnknownControl => "unknown_control",
            E::KeyNotAuthorized => "key_not_authorized",
            E::RevokedBeyondLastValid => "revoked_beyond_last_valid",
            E::BadChain => "bad_chain",
        }),
    }
}

fn publication_view(publication: &Publication) -> Value {
    let head = publication.feed.head();
    let conflicts = publication
        .feed
        .conflicts()
        .iter()
        .map(|(first, second)| [hex(first), hex(second)])
        .collect::<Vec<_>>();
    let frozen = publication.feed.is_frozen();
    let provenance = if frozen || !conflicts.is_empty() {
        "conflict"
    } else if head.is_some() {
        "confirmed"
    } else {
        "checking"
    };
    let mut posts: Vec<_> = publication.posts.iter().collect();
    posts.sort_by_key(|(id, post)| (std::cmp::Reverse(post.published_at_ms), *id));
    let posts: Vec<_> = posts
        .into_iter()
        .filter_map(|(id, post)| {
            use publication::{Kind, PostView};
            let (state, text, reason) = match publication.feed.post(id)? {
                PostView::Live(Kind::Post) => ("published", post.content.as_deref(), None),
                PostView::Live(Kind::Edit) => ("edited", post.content.as_deref(), None),
                PostView::Live(Kind::Tombstone) | PostView::Retracted => {
                    ("retracted", None, post.content.as_deref())
                }
            };
            Some(json!({
                "id": hex(id), "publishedAtMs": post.published_at_ms, "state": state,
                "text": text, "reason": reason,
            }))
        })
        .collect();
    json!({
        "head": head.map(|h| json!({
            "hash": hex(&h.hash), "version": h.version.to_string(), "discussion": h.discussion.as_deref().map(hex),
        })),
        "frozen": frozen, "provenance": provenance, "conflicts": conflicts, "posts": posts,
    })
}

fn space_state(
    space: &mut Space,
    accounts: &[ControllerAccount],
    signed: &str,
) -> Result<&'static str, &'static str> {
    let (body, entries) = envelope(signed, 131072)?;
    let state = space_control::SpaceState::from_body(&body).map_err(|_| "malformed")?;
    let approvers = control_approvers(accounts, SPACE_CONTROL, &body, &entries)?;
    if state.version == u64::MAX {
        // The Core reducer checks successors with version + 1.
        return Err("malformed");
    }
    if space.controls >= MAX_CONTROL_STATES {
        return Err("quota");
    }
    use space_control::{ControlError as E, ControlOutcome as O};
    match space.chain.accept(state, &approvers) {
        Ok(O::Duplicate) => Ok("duplicate"),
        Ok(result) => {
            space.controls += 1;
            Ok(if result == O::Forked {
                "forked"
            } else {
                "accepted"
            })
        }
        Err(error) => Err(match error {
            E::WrongSpace => "wrong_space",
            E::MissingPrev => "missing_prev",
            E::UnknownPrev => "unknown_prev",
            E::BadVersion => "bad_version",
            E::Governance => "governance",
            E::NotAuthorized => "not_authorized",
            E::Ceiling => "ceiling",
            E::Protected => "protected",
            E::Rollback => "rollback",
            E::Frozen => "frozen",
        }),
    }
}

fn space_view(space: &Space, reader: Option<[u8; 32]>, live: Option<Vec<String>>) -> Value {
    let live = match live {
        Some(ids) if ids.len() <= 64 => {
            let Some(principals) = ids
                .iter()
                .map(|id| fixed_hex::<32>(id).map(|id| (KIND_ACCOUNT, id.to_vec())))
                .collect::<Option<BTreeSet<_>>>()
            else {
                return refused("malformed");
            };
            Some(principals)
        }
        Some(_) => return refused("malformed"),
        None => None,
    };
    let head = space.chain.head().filter(|_| !space.chain.is_frozen());
    let governance = if space.chain.is_frozen() {
        Some("conflict")
    } else {
        head.zip(live.as_ref()).map(|(h, live)| {
            if h.governance_reachable(live) {
                "active"
            } else {
                "orphaned"
            }
        })
    };
    json!({
        "governance": governance,
        "head": head.map(|head| json!({
            "hash": hex(&head.hash),
            "version": head.version.to_string(),
            "member": reader.map(|id| head.members.contains_key(&(KIND_ACCOUNT, id.to_vec()))),
            "governanceThreshold": head.threshold,
            "links": head.links.iter().filter_map(|(kind, id)| {
                let kind = match kind { 1 => "room", 2 => "publication", 3 => "community", _ => return None };
                Some(json!({ "kind": kind, "id": hex(id) }))
            }).collect::<Vec<_>>(),
        })),
    })
}

impl Handles {
    pub fn close_profile(&mut self, profile: &str) {
        self.publications.retain(|(id, _), _| id != profile);
        self.spaces.retain(|(id, _), _| id != profile);
        self.directories.remove(profile);
    }

    pub fn publication_do(
        &mut self,
        ctx: &ProfileExecutionContext,
        handle: &str,
        op: PublicationOp,
        accounts: &[ControllerAccount],
    ) -> Value {
        let Some(id) = fixed_hex(handle) else {
            return refused("malformed");
        };
        let key = (ctx.profile_id.clone(), id);
        match op {
            PublicationOp::Open => {
                if self.publications.len() >= MAX_HANDLES && !self.publications.contains_key(&key) {
                    return refused("too_many_handles");
                }
                self.publications.entry(key).or_insert_with(|| Publication {
                    feed: Feed::new(id),
                    controls: 0,
                    events: 0,
                    event_bytes: 0,
                    held: BTreeMap::new(),
                    posts: BTreeMap::new(),
                });
                done()
            }
            PublicationOp::Close => json!({ "done": self.publications.remove(&key).is_some() }),
            op => {
                let Some(publication) = self.publications.get_mut(&key) else {
                    return refused("unknown_handle");
                };
                match op {
                    PublicationOp::AcceptControl { signed } => {
                        publication_control(publication, accounts, &signed)
                            .map_or_else(refused, outcome)
                    }
                    PublicationOp::AcceptEvent { signed, public_key } => {
                        publication_event(publication, &signed, &public_key)
                            .map_or_else(refused, outcome)
                    }
                    PublicationOp::View => publication_view(publication),
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn space_do(
        &mut self,
        ctx: &ProfileExecutionContext,
        handle: &str,
        op: SpaceOp,
        accounts: &[ControllerAccount],
        reader: Option<[u8; 32]>,
    ) -> Value {
        let Some(id) = fixed_hex(handle) else {
            return refused("malformed");
        };
        let key = (ctx.profile_id.clone(), id);
        match op {
            SpaceOp::Open => {
                if self.spaces.len() >= MAX_HANDLES && !self.spaces.contains_key(&key) {
                    return refused("too_many_handles");
                }
                self.spaces.entry(key).or_insert_with(|| Space {
                    chain: SpaceChain::new(id),
                    controls: 0,
                });
                done()
            }
            SpaceOp::Close => json!({ "done": self.spaces.remove(&key).is_some() }),
            op => {
                let Some(space) = self.spaces.get_mut(&key) else {
                    return refused("unknown_handle");
                };
                match op {
                    SpaceOp::AcceptState { signed } => {
                        space_state(space, accounts, &signed).map_or_else(refused, outcome)
                    }
                    SpaceOp::View { live_accounts } => space_view(space, reader, live_accounts),
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn directory_do(&mut self, ctx: &ProfileExecutionContext, op: DirectoryOp) -> Value {
        if !self.directories.contains_key(&ctx.profile_id) {
            if !matches!(&op, DirectoryOp::Observe { .. }) {
                return match op {
                    DirectoryOp::List => json!({ "entries": [] }),
                    DirectoryOp::Withdraw { .. } => json!({ "done": false }),
                    _ => refused("unknown_place"),
                };
            }
            if self.directories.len() >= MAX_HANDLES {
                return refused("too_many_handles");
            }
        }
        let directory = self.directories.entry(ctx.profile_id.clone()).or_default();
        let result = match op {
            DirectoryOp::List => {
                return json!({ "entries": directory.list().map(directory_entry).collect::<Vec<_>>() });
            }
            DirectoryOp::Observe {
                place,
                id,
                name,
                description,
            } => parse_place(place, &id)
                .and_then(|place| directory.observe(place, &name, description.as_deref()))
                .map(|()| done()),
            DirectoryOp::Withdraw { place, id } => {
                parse_place(place, &id).map(|place| json!({ "done": directory.withdraw(place) }))
            }
            DirectoryOp::ObserveOffer {
                place,
                id,
                incarnation,
                approval_required,
                uses,
                expires_at_ms,
            } => {
                if expires_at_ms.is_some_and(|ms| ms > MAX_SAFE_INTEGER) {
                    return refused("malformed");
                }
                parse_place(place, &id)
                    .and_then(|place| {
                        directory.observe_offer(
                            place,
                            directory::Terms {
                                incarnation: fixed_hex(&incarnation)
                                    .ok_or(directory::Error::Malformed)?,
                                approval_required,
                                uses: uses.map(Into::into),
                                expires_at_ms,
                            },
                        )
                    })
                    .map(|target| json!({ "target": hex(&target) }))
            }
            DirectoryOp::WithdrawOffer { place, id, target } => parse_place(place, &id)
                .and_then(|place| {
                    directory.withdraw_offer(
                        place,
                        fixed_hex(&target).ok_or(directory::Error::Malformed)?,
                    )
                })
                .map(|()| done()),
        };
        result.unwrap_or_else(|error| {
            refused(match error {
                directory::Error::Limit => "quota",
                directory::Error::Malformed => "malformed",
                directory::Error::UnknownPlace => "unknown_place",
                directory::Error::StaleTarget => "stale_target",
            })
        })
    }
}

fn parse_place(kind: PlaceKind, id: &str) -> Result<directory::Place, directory::Error> {
    Ok((
        match kind {
            PlaceKind::Community => directory::Kind::Community,
            PlaceKind::Room => directory::Kind::Room,
            PlaceKind::Publication => directory::Kind::Publication,
        },
        fixed_hex(id).ok_or(directory::Error::Malformed)?,
    ))
}

impl From<OfferUses> for directory::Uses {
    fn from(value: OfferUses) -> Self {
        match value {
            OfferUses::OneTime => Self::OneTime,
            OfferUses::Limited => Self::Limited,
            OfferUses::Reusable => Self::Reusable,
        }
    }
}

fn directory_entry(((kind, id), entry): (&directory::Place, &directory::Entry)) -> Value {
    json!({
        "kind": match kind {
            directory::Kind::Community => "community",
            directory::Kind::Room => "room",
            directory::Kind::Publication => "publication",
        },
        "id": hex(id), "name": entry.name, "description": entry.description, "source": "host_observed",
        "offer": entry.offer.as_ref().map(|offer| json!({
            "target": hex(&offer.target), "approvalRequired": offer.terms.approval_required,
            "expiresAtMs": offer.terms.expires_at_ms,
            "uses": offer.terms.uses.map(|uses| match uses {
                directory::Uses::OneTime => "one_time",
                directory::Uses::Limited => "limited",
                directory::Uses::Reusable => "reusable",
            }),
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use om_core::crypto::binding::{Acs, Credential};
    use om_core::crypto::sig::DeviceSigner;
    use std::path::Path;

    const ACCOUNT: [u8; 32] = [0x11; 32];
    const DEVICE: [u8; 16] = [7; 16];
    const OBJECT: [u8; 32] = [0xaa; 32];

    fn bodies() -> Value {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../verification/prototypes/v008-space-wire");
        let output = std::process::Command::new("python3").args([
            "-I", "-c", "import json,sys;sys.path.insert(0,sys.argv[1]);import python_codec as c;v=json.load(open(sys.argv[1]+'/vectors.json'));print(json.dumps({x['id']:c.enc(c.norm(x['value'])).hex() for x in v['positive']}))",
        ]).arg(dir).output().expect("V-008 corpus encoder");
        assert!(output.status.success());
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn body(vectors: &Value, id: &str) -> Vec<u8> {
        unhex(vectors[id].as_str().unwrap()).unwrap()
    }

    fn account(signer: &DeviceSigner) -> ControllerAccount {
        let credential = Credential {
            device_id: DEVICE,
            account_id: ACCOUNT,
            signing_pk: signer.public(),
            ik: [8; 32],
            capabilities: vec![],
        };
        ControllerAccount {
            head: Acs {
                account_id: ACCOUNT,
                generation: 1,
                control: BTreeSet::from([DEVICE]),
                quarantined: BTreeSet::new(),
                messaging: BTreeSet::from([DEVICE]),
                revoked: BTreeSet::new(),
                credentials: vec![(DEVICE, credential.hash())],
                previous: None,
                constitutional: (1, None),
                recovery_commitment: None,
                min_profile: 1,
                key_epochs: vec![],
                transparency: None,
            },
            credentials: vec![credential.body()],
        }
    }

    fn signed(label: &str, body: &[u8], role: u64, id: &[u8], key: &DeviceSigner) -> String {
        let signature = key.sign_with(&sig::ds_sig(label, PROFILE_V1, role, id, body), [9; 32]);
        hex(&sig::envelope(
            body.to_vec(),
            vec![sig::signature_entry(id, PROFILE_V1, role, signature)],
        ))
    }

    fn with_fields(body: &[u8], fields: impl IntoIterator<Item = (u64, V)>) -> Vec<u8> {
        let V::M(mut map) = cbor::decode(body).unwrap() else {
            panic!("expected a map")
        };
        for (key, value) in fields {
            map.retain(|(old, _)| old != &key);
            map.push((key, value));
        }
        cbor::encode(&V::M(map))
    }

    fn ctx() -> ProfileExecutionContext {
        ProfileExecutionContext {
            profile_id: "test".into(),
            generation: "one".into(),
        }
    }

    fn test_signer(seed: u8) -> DeviceSigner {
        DeviceSigner {
            ed_seed: [seed; 32],
            mldsa_seed: [seed + 1; 32],
        }
    }

    #[test]
    fn signed_space_vectors_use_prior_state_and_report_governance() {
        let vectors = bodies();
        let signer = test_signer(1);
        let accounts = [account(&signer)];
        let mut handles = Handles::default();
        let ctx = ctx();
        let id = hex(&OBJECT);
        assert_eq!(
            handles.space_do(&ctx, &id, SpaceOp::Open, &[], None),
            done()
        );
        let genesis = body(&vectors, "space-minimal");
        let populated = body(&vectors, "space-populated");
        let accept = |handles: &mut Handles, body: &[u8], accounts: &[ControllerAccount]| {
            let signed = signed(SPACE_CONTROL, body, sig::ROLE_CONTROLLER, &DEVICE, &signer);
            handles.space_do(&ctx, &id, SpaceOp::AcceptState { signed }, accounts, None)
        };
        assert_eq!(
            accept(&mut handles, &genesis, &[]),
            refused("unknown_signer")
        );
        assert_eq!(
            accept(&mut handles, &genesis, &accounts),
            outcome("accepted")
        );
        assert_eq!(
            accept(&mut handles, &genesis, &accounts),
            outcome("duplicate")
        );
        assert_eq!(
            accept(&mut handles, &populated, &accounts),
            refused("unknown_prev")
        );
        let view = |handles: &mut Handles, live_accounts| {
            handles.space_do(
                &ctx,
                &id,
                SpaceOp::View { live_accounts },
                &[],
                Some(ACCOUNT),
            )
        };
        assert_eq!(view(&mut handles, Some(vec![]))["governance"], "orphaned");
        assert_eq!(
            view(&mut handles, Some(vec![hex(&ACCOUNT)]))["governance"],
            "active"
        );
        assert_eq!(view(&mut handles, None)["head"]["member"], true);

        let hash = sig::ds_hash(space_control::LBL_CONTROL_HASH, &genesis);
        let next = with_fields(&populated, [(3, V::U(2)), (4, sig::digest(&hash))]);
        let other = with_fields(&next, [(5, V::M(vec![(0, V::T("Other".into()))]))]);
        assert_eq!(accept(&mut handles, &next, &accounts), outcome("accepted"));
        assert_eq!(
            accept(&mut handles, &genesis, &accounts),
            refused("rollback")
        );
        assert_eq!(accept(&mut handles, &other, &accounts), outcome("forked"));
        assert_eq!(
            view(&mut handles, Some(vec![hex(&ACCOUNT)])),
            json!({ "head": null, "governance": "conflict" })
        );
        assert_eq!(
            handles.space_do(&ctx, &id, SpaceOp::Close, &[], None),
            done()
        );
        assert_eq!(view(&mut handles, None), refused("unknown_handle"));
    }

    #[test]
    fn signed_publication_vectors_project_only_applied_events_and_show_conflicts() {
        let vectors = bodies();
        let signer = test_signer(1);
        let publisher = test_signer(3);
        let pk = publisher.public();
        let key_ref = sha256(&[&pk]);
        let accounts = [account(&signer)];
        let mut handles = Handles::default();
        let ctx = ctx();
        let id = hex(&OBJECT);
        assert_eq!(
            handles.publication_do(&ctx, &id, PublicationOp::Open, &[]),
            done()
        );
        let view =
            |handles: &mut Handles| handles.publication_do(&ctx, &id, PublicationOp::View, &[]);
        assert_eq!(view(&mut handles)["provenance"], "checking");
        let control = body(&vectors, "publication-control");
        let frozen_control = signed(
            PUBLICATION_CONTROL,
            &control,
            sig::ROLE_CONTROLLER,
            &DEVICE,
            &signer,
        );
        assert_eq!(
            handles.publication_do(
                &ctx,
                &id,
                PublicationOp::AcceptControl {
                    signed: frozen_control
                },
                &accounts
            ),
            outcome("accepted")
        );
        assert_eq!(
            handles.publication_do(&ctx, &id, PublicationOp::Close, &[]),
            done()
        );
        assert_eq!(
            handles.publication_do(&ctx, &id, PublicationOp::Open, &[]),
            done()
        );
        let control = with_fields(
            &control,
            [(
                7,
                V::A(vec![V::M(vec![(0, V::B(key_ref.to_vec())), (1, V::U(1))])]),
            )],
        );
        let signed_control = signed(
            PUBLICATION_CONTROL,
            &control,
            sig::ROLE_CONTROLLER,
            &DEVICE,
            &signer,
        );
        let accept_control = |handles: &mut Handles, signed: String| {
            handles.publication_do(
                &ctx,
                &id,
                PublicationOp::AcceptControl { signed },
                &accounts,
            )
        };
        assert_eq!(
            accept_control(&mut handles, signed_control),
            outcome("accepted")
        );
        assert_eq!(view(&mut handles)["provenance"], "confirmed");
        let control_hash = sig::ds_hash(publication::LBL_CONTROL_HASH, &control);

        let frozen_post = body(&vectors, "publication-post");
        let post = with_fields(
            &frozen_post,
            [
                (7, V::B(key_ref.to_vec())),
                (10, sig::digest(&control_hash)),
            ],
        );
        let post_hash = sig::ds_hash(publication::LBL_EVENT_HASH, &post);
        let accept = |handles: &mut Handles, body: &[u8], public_key: &[u8]| {
            handles.publication_do(
                &ctx,
                &id,
                PublicationOp::AcceptEvent {
                    signed: signed(
                        PUBLICATION_EVENT,
                        body,
                        ROLE_PUBLICATION,
                        &key_ref,
                        &publisher,
                    ),
                    public_key: hex(public_key),
                },
                &[],
            )
        };
        assert_eq!(
            accept(&mut handles, &frozen_post, &pk),
            refused("unverifiable_key")
        );
        assert_eq!(
            accept(&mut handles, &post, &signer.public()),
            refused("unverifiable_key")
        );

        let edit = with_fields(
            &post,
            [
                (5, V::U(2)),
                (8, V::U(1)),
                (9, sig::digest(&post_hash)),
                (6, V::M(vec![(0, om_core::events::rich_text("Edited"))])),
            ],
        );
        assert_eq!(accept(&mut handles, &edit, &pk), outcome("held"));
        assert_eq!(view(&mut handles)["posts"], json!([]));
        assert_eq!(accept(&mut handles, &post, &pk), outcome("accepted"));
        assert_eq!(accept(&mut handles, &post, &pk), outcome("duplicate"));
        assert_eq!(view(&mut handles)["posts"][0]["state"], "edited");
        assert_eq!(view(&mut handles)["posts"][0]["text"], "Edited");
        let retract = with_fields(
            &edit,
            [
                (5, V::U(3)),
                (6, V::M(vec![(0, V::T("Withdrawn".into()))])),
                (8, V::U(2)),
                (
                    9,
                    sig::digest(&sig::ds_hash(publication::LBL_EVENT_HASH, &edit)),
                ),
            ],
        );
        assert_eq!(accept(&mut handles, &retract, &pk), outcome("accepted"));
        assert_eq!(view(&mut handles)["posts"][0]["state"], "retracted");
        assert_eq!(view(&mut handles)["posts"][0]["reason"], "Withdrawn");
        assert_eq!(view(&mut handles)["posts"][0]["text"], Value::Null);

        let equivocation = with_fields(
            &post,
            [(6, V::M(vec![(0, om_core::events::rich_text("Different"))]))],
        );
        assert_eq!(
            accept(&mut handles, &equivocation, &pk),
            outcome("equivocation")
        );
        assert_eq!(view(&mut handles)["provenance"], "conflict");
        assert_eq!(view(&mut handles)["conflicts"].as_array().unwrap().len(), 1);
        assert_eq!(view(&mut handles)["posts"][0]["state"], "retracted");
    }
}
