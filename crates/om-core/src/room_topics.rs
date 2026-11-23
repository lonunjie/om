// SPDX-License-Identifier: MPL-2.0
//! Room topics and organization (`communities_publications` slice 4; SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md
//! "Persistent Topics", "Chat mode vs forum mode", "Moving messages between topics"; ADR-0121 rule 7 table).
//!
//! Pure reducers over already-authenticated, already-ordered events. The caller supplies the actor's Room capabilities as of
//! the event's epoch and the event clock; nothing here reads a clock, I/O or provider state.
//!
//! * `topic.create` needs `room.create_topic`; `topic.update` and `topic.status.set` need the topic's creator or
//!   `room.manage_topics`; `message.topic.set` needs the message's author (still holding `room.post`) or `room.manage_topics`.
//! * A topic has no roster or encryption context. Moving a message changes placement only: the EventID, author and
//!   content are untouched, and a move to another Room is not this operation.
//! * Mutable fields (title, status, a message's topic) converge last-writer-wins on `(clock, event_id)`.
//! * `forum` requires a topic on a new top-level post; `announcement` is a hint whose real effect comes from `room.post`
//!   being restricted ([`announcement_is_effective`]). Follow and mute are local attention state ([`Attention`]) and never
//!   enter the shared state.

use std::collections::{BTreeMap, BTreeSet};

pub const CREATE_TOPIC: &str = "room.create_topic";
pub const MANAGE_TOPICS: &str = "room.manage_topics";
pub const POST: &str = "room.post";

pub type TopicId = [u8; 16];
pub type EventId = [u8; 32];
/// `(hlc clock, event id)`: the total order mutable fields converge on.
pub type Stamp = (u64, EventId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Open = 1,
    Resolved = 2,
    Archived = 3,
}

impl Status {
    pub fn from_wire(n: u64) -> Option<Self> {
        match n {
            1 => Some(Self::Open),
            2 => Some(Self::Resolved),
            3 => Some(Self::Archived),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Chat,
    Forum,
    Announcement,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TopicError {
    /// The actor lacks the capability or authorship the operation needs.
    NotAuthorized,
    /// `topic.create` for an id that exists.
    Duplicate,
    UnknownTopic,
    UnknownMessage,
    /// Title is empty or longer than 256 bytes, or the status is out of range.
    Malformed,
    /// Forum mode: a new top-level post carries no topic.
    TopicRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topic {
    pub title: String,
    pub status: Status,
    pub created_by: Vec<u8>,
    pub created: Stamp,
    title_at: Stamp,
    status_at: Stamp,
}

#[derive(Clone, Debug)]
struct Placed {
    author: Vec<u8>,
    topic: Option<TopicId>,
    at: Option<Stamp>,
}

#[derive(Clone, Debug, Default)]
pub struct RoomTopics {
    topics: BTreeMap<TopicId, Topic>,
    messages: BTreeMap<EventId, Placed>,
}

fn can(caps: &BTreeSet<String>, c: &str) -> bool {
    caps.contains(c)
}
fn title_ok(t: &str) -> bool {
    !t.is_empty() && t.len() <= 256
}

impl RoomTopics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a message so it can be moved later. `topic` is its placement at creation.
    /// Forum mode refuses a top-level post without one; a reply or thread reply inherits its root's placement, so the
    /// caller passes `is_top_level = false` for those.
    pub fn post(
        &mut self,
        mode: Mode,
        is_top_level: bool,
        event: EventId,
        author: Vec<u8>,
        topic: Option<TopicId>,
    ) -> Result<(), TopicError> {
        if mode == Mode::Forum && is_top_level && topic.is_none() {
            return Err(TopicError::TopicRequired);
        }
        if topic.is_some_and(|t| !self.topics.contains_key(&t)) {
            return Err(TopicError::UnknownTopic);
        }
        self.messages.insert(
            event,
            Placed {
                author,
                topic,
                at: None,
            },
        );
        Ok(())
    }

    pub fn create(
        &mut self,
        caps: &BTreeSet<String>,
        actor: Vec<u8>,
        id: TopicId,
        title: &str,
        at: Stamp,
    ) -> Result<(), TopicError> {
        if !can(caps, CREATE_TOPIC) {
            return Err(TopicError::NotAuthorized);
        }
        if !title_ok(title) {
            return Err(TopicError::Malformed);
        }
        if self.topics.contains_key(&id) {
            return Err(TopicError::Duplicate);
        }
        self.topics.insert(
            id,
            Topic {
                title: title.into(),
                status: Status::Open,
                created_by: actor,
                created: at,
                title_at: at,
                status_at: at,
            },
        );
        Ok(())
    }

    fn manage(
        &self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        id: &TopicId,
    ) -> Result<(), TopicError> {
        let t = self.topics.get(id).ok_or(TopicError::UnknownTopic)?;
        if t.created_by == actor || can(caps, MANAGE_TOPICS) {
            Ok(())
        } else {
            Err(TopicError::NotAuthorized)
        }
    }

    pub fn update(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        id: TopicId,
        title: &str,
        at: Stamp,
    ) -> Result<(), TopicError> {
        self.manage(caps, actor, &id)?;
        if !title_ok(title) {
            return Err(TopicError::Malformed);
        }
        let t = self.topics.get_mut(&id).unwrap();
        if at > t.title_at {
            t.title = title.into();
            t.title_at = at;
        }
        Ok(())
    }

    pub fn set_status(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        id: TopicId,
        status: u64,
        at: Stamp,
    ) -> Result<(), TopicError> {
        self.manage(caps, actor, &id)?;
        let s = Status::from_wire(status).ok_or(TopicError::Malformed)?;
        let t = self.topics.get_mut(&id).unwrap();
        if at > t.status_at {
            t.status = s;
            t.status_at = at;
        }
        Ok(())
    }

    /// `message.topic.set`: the author (who must still hold `room.post`) or a `room.manage_topics` holder. Absent
    /// `topic` clears the placement. EventID, author and content are untouched.
    pub fn move_message(
        &mut self,
        caps: &BTreeSet<String>,
        actor: &[u8],
        message: EventId,
        topic: Option<TopicId>,
        at: Stamp,
    ) -> Result<(), TopicError> {
        let m = self
            .messages
            .get(&message)
            .ok_or(TopicError::UnknownMessage)?;
        let author_ok = m.author == actor && can(caps, POST);
        if !(author_ok || can(caps, MANAGE_TOPICS)) {
            return Err(TopicError::NotAuthorized);
        }
        if topic.is_some_and(|t| !self.topics.contains_key(&t)) {
            return Err(TopicError::UnknownTopic);
        }
        let m = self.messages.get_mut(&message).unwrap();
        if m.at.is_none_or(|prev| at > prev) {
            m.topic = topic;
            m.at = Some(at);
        }
        Ok(())
    }

    pub fn topic(&self, id: &TopicId) -> Option<&Topic> {
        self.topics.get(id)
    }
    pub fn placement(&self, message: &EventId) -> Option<Option<TopicId>> {
        self.messages.get(message).map(|m| m.topic)
    }
    pub fn author_of(&self, message: &EventId) -> Option<&[u8]> {
        self.messages.get(message).map(|m| m.author.as_slice())
    }
}

/// `announcement` is a presentation hint: it is effective only when `room.post` is withheld from ordinary members, so a
/// Room labelled announcement whose members can all post is just a chat Room.
pub fn announcement_is_effective(ordinary_member_caps: &BTreeSet<String>) -> bool {
    !ordinary_member_caps.contains(POST)
}

/// Follow and mute: the member's own attention, local and never part of the shared Room state.
#[derive(Clone, Debug, Default)]
pub struct Attention {
    followed: BTreeSet<TopicId>,
    muted: BTreeSet<TopicId>,
}

impl Attention {
    pub fn follow(&mut self, t: TopicId) {
        self.muted.remove(&t);
        self.followed.insert(t);
    }
    pub fn mute(&mut self, t: TopicId) {
        self.followed.remove(&t);
        self.muted.insert(t);
    }
    pub fn clear(&mut self, t: &TopicId) {
        self.followed.remove(t);
        self.muted.remove(t);
    }
    pub fn is_followed(&self, t: &TopicId) -> bool {
        self.followed.contains(t)
    }
    pub fn is_muted(&self, t: &TopicId) -> bool {
        self.muted.contains(t)
    }
}
