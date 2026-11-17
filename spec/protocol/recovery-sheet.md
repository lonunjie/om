# Recovery sheet string v1

- **Status:** Normative W1 interface (ADR-0200). Additive: nothing in [`RECOVERY_AND_BACKUP_POLICY.md`](../../docs/architecture/identity/RECOVERY_AND_BACKUP_POLICY.md) or any frozen W0 object changes.
- **Carries:** the 256-bit RecoveryRoot, the Home locator and the `recovery_scope_id` of the RecoveryStore, in one checksummed string, so that nobody types a URL.
- **Not a CBOR object.** It is a transcription and scanning format. The same bytes travel as text (section 3) and as a QR (section 4).

## 1. Binary layout

All integers are big-endian. There is no padding and no optional field.

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | `version`, always `0x01` |
| 1 | 1 | `kind`: `0x01` full (secret), `0x02` place only (not secret) |
| 2 | 32 | `root`, the RecoveryRoot. **Present only when `kind = 0x01`.** |
| next | 32 | `scope_id`, the `recovery_scope_id` the Home advertises |
| next | 2 | `port`; `0` means 443 |
| next | 1 | `host_len`, 1 to 253 |
| next | `host_len` | `host`, ASCII |
| last | 4 | `check` |

`host` is a lowercase ASCII hostname (an internationalized name in its punycode form). Each label is 1 to 63 characters of
`a-z`, `0-9` or `-`, not starting or ending with `-`. There is at least one dot, no trailing dot, and the last label is not
all digits (so no IP address and no bare `localhost`). The scheme is always `https`. A Home that is not reachable as
`https://host[:port]/` at the origin root cannot be named by a sheet; it is reached through a redirect record (ADR-0200 section 6).

`check` is the first 4 bytes of the digest value (the `Digest` key 1) of `DS-HASH(OM-RECOVERY-SHEET-CHECK-v1, body)`, where
`body` is every byte before `check`. A parser rejects, with nothing used: an unknown `version` or `kind`, a length that does not match
`kind` and `host_len`, a `host` that breaks the rules above, trailing bytes, and a `check` that does not match (compared in constant time).

A `kind = 0x01` string is a secret of class `om.recovery_secret`: it is shown only on a secure surface, copied only with the
clipboard's sensitive-write flag, never written to a log, a crash report or a backup, and never sent to a Home. A `kind = 0x02`
string holds no secret and is not secret-handled; it names where a record lives.

Sizes: `kind 0x01` is 73 + `host_len` bytes (103 bytes for a 30-character host), `kind 0x02` is 41 + `host_len` bytes.

## 2. Legacy key-only form

A string that decodes (section 3) to exactly 35 bytes is the earlier key-only form: 32 bytes of RecoveryRoot followed by 3 bytes of
`sha256("OM-RECOVERY-KEY-DISPLAY-CHECK" || root)`. It is still accepted for entry and gives a root with no place. The place then
comes from a `kind = 0x02` string (ADR-0200 section 6). New sheets are never written in the legacy form.

## 3. Text form

The text form is the RFC 4648 base32 alphabet (`A-Z`, `2-7`), no padding, of the whole binary layout including `check`.

- **Compact form** (clipboard, password manager, file): the characters only. A parser ignores case, spaces, hyphens and line breaks.
- **Sheet form** (printed or shown for transcription): groups of 4 characters separated by a space, **6 groups per line**, and after each
  line a colon and a **line check** of 2 characters: `ABCD EFGH IJKL MNOP QRST UVWX : YZ`. The last line holds the remaining groups (its last group may be shorter).
  The line check is the first 10 bits, as two base32 characters, of the digest value of `DS-HASH(OM-RECOVERY-SHEET-LINE-CHECK-v1, body)`
  where `body` is the line index (1 byte, from 0) followed by the line's payload characters in upper case with no separators.
  A parser that sees a colon verifies the two characters after it against the line before them and rejects the string on a mismatch,
  which lets an entry screen say "line 3 does not match" while a person is typing. Line checks are an aid; the whole-string `check` decides.
- An entry screen MAY offer one correction of a confusable character (`0` for `O`, `1` for `I` or `L`, `8` for `B`) and only when exactly one
  single-character substitution makes both the line check and `check` pass. It never guesses silently.

## 4. QR form

The QR content is the ASCII text `om:recover/` followed by the base64url (RFC 4648 section 5, no padding) of the binary layout. The QR
follows [`INVITE_QR_MEDIA.md`](../../docs/ux-design/specs/INVITE_QR_MEDIA.md) (error correction M, a quiet zone of at least 4 modules, a light plate, no logo overlay); the sheet prints the sheet-form text
beside it as the fallback, and the QR's accessible description states its purpose without transcribing the encoded value.
A `kind = 0x01` QR is secret-handled like the text. A scan that does not parse is rejected without echoing its content.

## 5. After parsing

A `kind = 0x01` or `0x02` result is used only after the client fetches `https://host[:port]/.well-known/om-provider-capabilities` and finds
`recovery_scope_id` equal to `scope_id` and the feature `om.recovery.store.v1`. A different `recovery_scope_id` stops the attempt with
"this address is out of date" and moves to the fallback of ADR-0200 section 6; it is never overridden by the Home's value.
