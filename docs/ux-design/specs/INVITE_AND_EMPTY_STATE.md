# Invite-First Empty State

**Status:** Normative first-run direction

## 1. Privacy-first starting point

OM must not assume address-book upload or social-graph discovery as the default path to usefulness.

A newly installed client may legitimately have:

- no contacts;
- no conversations;
- no uploaded address book;
- no public username directory configured.

This is a normal state, not an error to be “fixed” through aggressive permission prompts.

## 2. First useful screen

After identity/setup requirements are satisfied, the initial root remains the chat list.

When empty, the primary call to action should be invitation-oriented and human rather than technical, for example:

> **Invite someone to chat**

Supporting copy may mention friends, family, workmates, classmates, or anyone the user already knows without implying that OM wants access to those relationships.

The empty state should feel welcoming and lightly playful through OhMessenger, while preserving fast task completion.

## 3. Invitation paths

From the empty state or Omnibar, the user should be able to:

- show a QR code for direct scanning;
- scan another person's QR code;
- create/share an invite link;
- share through an already installed messaging app;
- share through email;
- copy the link/code;
- choose a one-time invite intended for one acceptance;
- create separate one-time invitations for different people;
- choose a reusable/expiring invite where the architecture permits it.

The UI should describe the behavioral difference between one-time and reusable invites without requiring users to understand resolver or cryptographic terminology.

## 4. Deep-link/install behavior

An invite link should aim for this user journey:

```text
recipient opens invite
      ↓
OM already installed? ── yes → open OM to bounded invite acceptance
      │
      no
      ↓
offer/install supported client (PWA first target)
      ↓
resume the same invite after installation
```

The invitation must survive the install handoff safely enough that the recipient does not need to re-enter a complex code when platform capabilities permit.

The architecture merge must validate the exact deep-link, expiry, anti-replay, and security behavior.

## 5. One-time invitation UX

A one-time invite should communicate simply:

- intended for one acceptance;
- whether it expires;
- whether it has been used;
- whether it has been revoked;
- what identity/account it connects to.

The UI must not imply stronger guarantees than the underlying architecture can prove.

## 6. No growth pressure

OM is not designed to maximize social-graph growth.

Do not:

- nag repeatedly for contacts permission;
- upload contacts by default;
- dark-pattern users into reusable/public discovery;
- gamify number of contacts;
- treat an empty network as failure;
- require a centralized address book to start communicating.

An optional address-book matching feature may exist later only under explicit privacy rules and is outside this default flow.

## 7. Empty-state evolution

Once conversations exist, the invitation CTA should recede naturally. Invite remains reachable through:

- Omnibar;
- People or equivalent view if enabled;
- conversation/group actions where relevant;
- QR scanner action;
- Action Registry/command palette.

The app should not keep permanent onboarding chrome after the user has learned the flow.

## 8. Delight without coercion

OhMessenger may make the first invite experience warm and memorable using restrained illustration, semantic motion, or celebratory feedback after a successful connection.

It must not obscure:

- invite scope;
- one-time/reusable status;
- expiry;
- recipient identity verification where applicable;
- privacy consequences.
