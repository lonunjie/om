// SPDX-License-Identifier: MPL-2.0
//! The recovery key sheet string (ADR-0200, `spec/protocol/recovery-sheet.md`): one checksummed string that carries the
//! 256-bit RecoveryRoot (full kind only), the `recovery_scope_id` and the Home origin, so that nobody types a URL.
//!
//! * Binary layout: `version(1) kind(1) [root(32)] scope(32) port(2, BE, 0 = 443) host_len(1) host check(4)`; `check` is the
//!   first 4 bytes of the digest value of `DS-HASH(OM-RECOVERY-SHEET-CHECK-v1, body)`.
//! * Text: RFC 4648 base32, no padding. Compact (case, spaces, hyphens and line breaks ignored) and sheet form (groups of 4,
//!   6 groups per line, each line closed by ` : ` and a 2-character line check of
//!   `DS-HASH(OM-RECOVERY-SHEET-LINE-CHECK-v1, line_index(1) || payload)`, an aid that says "line 3 does not match").
//! * QR: `om:recover/` + base64url of the binary layout.
//! * Legacy: a string that decodes to exactly 35 bytes is the earlier key-only form (root + 3 check bytes); accepted for
//!   entry, never produced.
//!
//! A full-kind string, its QR and the entered key are class `om.recovery_secret`: this module never prints one (the types
//! redact `Debug`) and wipes the root on drop. The string is not authenticated: a substituted string yields another root.

use crate::crypto::sig::ds_hash;
use zeroize::Zeroize;

pub const LBL_SHEET_CHECK: &str = "OM-RECOVERY-SHEET-CHECK-v1";
pub const LBL_LINE_CHECK: &str = "OM-RECOVERY-SHEET-LINE-CHECK-v1";
pub const QR_PREFIX: &str = "om:recover/";
pub const VERSION: u8 = 1;
pub const KIND_FULL: u8 = 1;
pub const KIND_PLACE: u8 = 2;
pub const GROUPS_PER_LINE: usize = 6;
pub const MAX_HOST: usize = 253;
/// Redirect hops a recovering device follows at most (ADR-0200 section 6).
pub const MAX_REDIRECT_HOPS: usize = 3;
const LEGACY_BYTES: usize = 35;
/// Nothing longer than the longest valid string (full kind, 253-byte host) is parsed.
const MAX_TEXT_CHARS: usize = 1024;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Why a string was refused. Never carries the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetError {
    /// A character outside the base32 alphabet (after ignoring case, spaces, hyphens and line breaks).
    Alphabet,
    /// Too long, too short, or a length that does not match the kind and `host_len`.
    Length,
    Version,
    Kind,
    Host,
    Trailing,
    /// The whole-string check did not match.
    Check,
    /// The line check of this line (1-based) did not match.
    LineCheck(usize),
    /// A QR payload without the `om:recover/` prefix, or with a bad base64url body.
    Qr,
}

impl SheetError {
    pub fn name(&self) -> &'static str {
        match self {
            SheetError::Alphabet => "alphabet",
            SheetError::Length => "length",
            SheetError::Version => "version",
            SheetError::Kind => "kind",
            SheetError::Host => "host",
            SheetError::Trailing => "trailing",
            SheetError::Check => "check",
            SheetError::LineCheck(_) => "line_check",
            SheetError::Qr => "qr",
        }
    }
}

/// A parsed sheet string: where the RecoveryStore is, and (full kind) the RecoveryRoot.
#[derive(Clone, PartialEq, Eq)]
pub struct Sheet {
    root: Option<[u8; 32]>,
    pub scope: [u8; 32],
    /// `None` = 443.
    pub port: Option<u16>,
    pub host: String,
}

impl core::fmt::Debug for Sheet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Sheet({}, <redacted>)",
            if self.root.is_some() { "full" } else { "place" }
        )
    }
}

impl Drop for Sheet {
    fn drop(&mut self) {
        if let Some(r) = &mut self.root {
            r.zeroize();
        }
    }
}

/// What a string parsed to.
#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Sheet(Sheet),
    /// The earlier key-only text: a root and no place. The place then comes from a `kind = 2` string.
    Legacy(LegacyKey),
}

#[derive(PartialEq, Eq)]
pub struct LegacyKey(pub [u8; 32]);

impl core::fmt::Debug for LegacyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("LegacyKey(<redacted>)")
    }
}

impl Drop for LegacyKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// ------------------------------------------------------------------------------------------------ host rules

/// The host rules of the sheet: lowercase ASCII (punycode for international names), labels of 1 to 63 characters of
/// `a-z 0-9 -` not starting or ending with `-`, at least one dot, no trailing dot, the last label not all digits (so no IP
/// address and no bare `localhost`), at most 253 bytes. The scheme is always `https`.
pub fn valid_host(host: &str) -> bool {
    if host.is_empty() || host.len() > MAX_HOST || !host.contains('.') {
        return false;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let ok = |l: &&str| {
        !l.is_empty()
            && l.len() <= 63
            && l.bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            && !l.starts_with('-')
            && !l.ends_with('-')
    };
    labels.iter().all(ok)
        && !labels
            .last()
            .is_some_and(|l| l.bytes().all(|c| c.is_ascii_digit()))
}

impl Sheet {
    /// A full-kind sheet: the secret root, plus the place.
    pub fn full(
        root: [u8; 32],
        scope: [u8; 32],
        port: Option<u16>,
        host: &str,
    ) -> Result<Sheet, SheetError> {
        Self::make(Some(root), scope, port, host)
    }

    /// A place-only sheet: holds no secret.
    pub fn place_only(scope: [u8; 32], port: Option<u16>, host: &str) -> Result<Sheet, SheetError> {
        Self::make(None, scope, port, host)
    }

    fn make(
        root: Option<[u8; 32]>,
        scope: [u8; 32],
        port: Option<u16>,
        host: &str,
    ) -> Result<Sheet, SheetError> {
        if !valid_host(host) {
            return Err(SheetError::Host);
        }
        Ok(Sheet {
            root,
            scope,
            port,
            host: host.into(),
        })
    }

    pub fn is_full(&self) -> bool {
        self.root.is_some()
    }

    /// The RecoveryRoot of a full-kind sheet, for deriving keys and nothing else.
    pub fn root(&self) -> Option<&[u8; 32]> {
        self.root.as_ref()
    }

    /// The same place without the secret (what Settings shows as the recovery address).
    pub fn place(&self) -> Sheet {
        Sheet {
            root: None,
            scope: self.scope,
            port: self.port,
            host: self.host.clone(),
        }
    }

    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(443)
    }

    /// `host` or `host:port`, the authority of `https://host[:port]/`.
    pub fn authority(&self) -> String {
        match self.port {
            None | Some(443) => self.host.clone(),
            Some(p) => format!("{}:{p}", self.host),
        }
    }

    /// The binary layout of section 1 (a secret when the kind is full: wipe it after use).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = vec![
            VERSION,
            if self.root.is_some() {
                KIND_FULL
            } else {
                KIND_PLACE
            },
        ];
        if let Some(r) = &self.root {
            b.extend_from_slice(r);
        }
        b.extend_from_slice(&self.scope);
        b.extend_from_slice(&self.port.unwrap_or(0).to_be_bytes());
        b.push(self.host.len() as u8);
        b.extend_from_slice(self.host.as_bytes());
        let chk = whole_check(&b);
        b.extend_from_slice(&chk);
        b
    }

    pub fn from_bytes(b: &[u8]) -> Result<Sheet, SheetError> {
        if b.len() < 2 {
            return Err(SheetError::Length);
        }
        if b[0] != VERSION {
            return Err(SheetError::Version);
        }
        let full = match b[1] {
            KIND_FULL => true,
            KIND_PLACE => false,
            _ => return Err(SheetError::Kind),
        };
        let fixed = 2 + if full { 32 } else { 0 } + 32 + 2 + 1;
        if b.len() < fixed + 4 {
            return Err(SheetError::Length);
        }
        let host_len = b[fixed - 1] as usize;
        if host_len == 0 || host_len > MAX_HOST {
            return Err(SheetError::Host);
        }
        match b.len().cmp(&(fixed + host_len + 4)) {
            core::cmp::Ordering::Less => return Err(SheetError::Length),
            core::cmp::Ordering::Greater => return Err(SheetError::Trailing),
            core::cmp::Ordering::Equal => {}
        }
        let (body, check) = b.split_at(b.len() - 4);
        if !ct_eq(check, &whole_check(body)) {
            return Err(SheetError::Check);
        }
        let mut at = 2;
        let root = if full {
            let r: [u8; 32] = b[at..at + 32].try_into().expect("32");
            at += 32;
            Some(r)
        } else {
            None
        };
        let scope: [u8; 32] = b[at..at + 32].try_into().expect("32");
        at += 32;
        let port = u16::from_be_bytes([b[at], b[at + 1]]);
        let host =
            core::str::from_utf8(&b[fixed..fixed + host_len]).map_err(|_| SheetError::Host)?;
        Sheet::make(root, scope, (port != 0).then_some(port), host)
    }

    /// Compact text form: upper-case base32 without separators (clipboard, password manager, file).
    pub fn to_compact(&self) -> String {
        let mut b = self.to_bytes();
        let s = base32(&b);
        b.zeroize();
        s
    }

    /// Sheet form for transcription: groups of 4, six per line, each line closed by ` : ` and its 2-character line check.
    pub fn to_sheet_form(&self) -> String {
        let compact = self.to_compact();
        sheet_form(&compact)
    }

    /// The QR content: `om:recover/` + base64url (no padding) of the binary layout.
    pub fn to_qr(&self) -> String {
        let mut b = self.to_bytes();
        let s = format!("{QR_PREFIX}{}", b64url(&b));
        b.zeroize();
        s
    }
}

fn whole_check(body: &[u8]) -> [u8; 4] {
    ds_hash(LBL_SHEET_CHECK, body)[..4].try_into().expect("4")
}

/// Constant-time comparison of two equal-length byte strings.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// The 2-character line check of line `index` (0-based) over its payload characters (upper case, no separators).
pub fn line_check(index: usize, payload: &str) -> String {
    let mut body = vec![index as u8];
    body.extend_from_slice(payload.as_bytes());
    let d = ds_hash(LBL_LINE_CHECK, &body);
    let v = ((d[0] as u16) << 2) | (d[1] as u16 >> 6);
    [ALPHABET[(v >> 5) as usize & 31], ALPHABET[v as usize & 31]]
        .iter()
        .map(|c| *c as char)
        .collect()
}

fn sheet_form(compact: &str) -> String {
    let chars: Vec<&str> = compact
        .as_bytes()
        .chunks(4)
        .map(|g| core::str::from_utf8(g).expect("ascii"))
        .collect();
    chars
        .chunks(GROUPS_PER_LINE)
        .enumerate()
        .map(|(i, groups)| format!("{} : {}", groups.join(" "), line_check(i, &groups.concat())))
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------------------------------------------- text parse

/// Parse what a person typed, pasted or scanned: the compact form, the sheet form (with or without line checks), a QR
/// payload (`om:recover/...`), a full or place-only string, or the legacy key-only text. Nothing is echoed on failure.
pub fn parse(text: &str) -> Result<Parsed, SheetError> {
    let t = text.trim();
    if let Some(rest) = t.strip_prefix(QR_PREFIX) {
        let mut b = unb64url(rest).ok_or(SheetError::Qr)?;
        let r = Sheet::from_bytes(&b).map(Parsed::Sheet);
        b.zeroize();
        return r;
    }
    let mut bytes = decode_text(t)?;
    let r = if bytes.len() == LEGACY_BYTES {
        parse_legacy(&bytes)
    } else {
        Sheet::from_bytes(&bytes).map(Parsed::Sheet)
    };
    bytes.zeroize();
    r
}

fn parse_legacy(b: &[u8]) -> Result<Parsed, SheetError> {
    let root: [u8; 32] = b[..32].try_into().expect("32");
    let chk = crate::crypto::sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", &root]);
    if ct_eq(&b[32..], &chk[..3]) {
        Ok(Parsed::Legacy(LegacyKey(root)))
    } else {
        Err(SheetError::Check)
    }
}

/// Characters to base32-decode, with every line check that is present verified on the way.
fn decode_text(t: &str) -> Result<Vec<u8>, SheetError> {
    let mut payload = String::new();
    let mut line = String::new();
    let mut line_index = 0usize;
    let mut chars = t.chars().filter(|c| !c.is_whitespace() && *c != '-');
    let mut count = 0usize;
    while let Some(c) = chars.next() {
        count += 1;
        if count > MAX_TEXT_CHARS {
            return Err(SheetError::Length);
        }
        if c == ':' {
            let want = line_check(line_index, &line);
            let got: String = chars
                .by_ref()
                .take(2)
                .map(|c| c.to_ascii_uppercase())
                .collect();
            if got != want {
                return Err(SheetError::LineCheck(line_index + 1));
            }
            line.clear();
            line_index += 1;
            continue;
        }
        let u = c.to_ascii_uppercase();
        if !u.is_ascii() || !ALPHABET.contains(&(u as u8)) {
            return Err(SheetError::Alphabet);
        }
        payload.push(u);
        line.push(u);
    }
    unbase32(&payload).ok_or(SheetError::Length)
}

/// One correction of a confusable character (`0` for `O`, `1` for `I` or `L`, `8` for `B`), offered only when exactly one
/// single-character substitution makes the whole string parse (the whole-string check and every line check pass). The
/// result is the corrected compact text, never applied silently.
pub fn suggest_correction(text: &str) -> Option<String> {
    let t = text.trim();
    if t.starts_with(QR_PREFIX) {
        return None;
    }
    let chars: Vec<char> = t.chars().collect();
    let mut found: Vec<String> = Vec::new();
    for (i, c) in chars.iter().enumerate() {
        let subs: &[char] = match c {
            '0' => &['O'],
            '1' => &['I', 'L'],
            '8' => &['B'],
            _ => continue,
        };
        for s in subs {
            let mut cand = chars.clone();
            cand[i] = *s;
            let cand: String = cand.into_iter().collect();
            if parse(&cand).is_ok() {
                found.push(cand);
            }
        }
    }
    if found.len() == 1 {
        let c: String = found
            .remove(0)
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        Some(c)
    } else {
        None
    }
}

/// Live feedback while typing: the first line (1-based) whose line check does not match, if any, over the complete lines
/// typed so far. `None` when every line with a check matches or none has one yet.
pub fn first_bad_line(text: &str) -> Option<usize> {
    let mut line = String::new();
    let mut idx = 0usize;
    let mut chars = text.chars().filter(|c| !c.is_whitespace() && *c != '-');
    while let Some(c) = chars.next() {
        if c == ':' {
            let got: String = chars
                .by_ref()
                .take(2)
                .map(|c| c.to_ascii_uppercase())
                .collect();
            if got.len() == 2 && got != line_check(idx, &line) {
                return Some(idx + 1);
            }
            line.clear();
            idx += 1;
        } else {
            line.push(c.to_ascii_uppercase());
        }
    }
    None
}

// ------------------------------------------------------------------------------------------------ redirect rules

/// Why a redirect (ADR-0200 section 6) is not followed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirectRefusal {
    /// More than [`MAX_REDIRECT_HOPS`] hops.
    TooManyHops,
    /// The new host breaks the host rules (including a non-https or IP-literal name).
    Host,
    /// The new place carries no `recovery_scope_id`.
    NoScope,
    /// This (host, scope) pair was already visited: a loop.
    Visited,
}

/// The checks of the redirect rule that need no network: the AEAD opened (the caller got the redirect from
/// `recovery_record::open`), the new place obeys the host rules, a scope is present, the (host, scope) pair is new and at
/// most 3 hops were taken. The capabilities-document check (it advertises the scope and `om.recovery.store.v1`) is the
/// caller's, after this passes.
pub fn check_redirect(
    new_place: &crate::recovery_record::ProviderHome,
    visited: &[(String, [u8; 32])],
    hops_taken: usize,
) -> Result<(), RedirectRefusal> {
    if hops_taken >= MAX_REDIRECT_HOPS {
        return Err(RedirectRefusal::TooManyHops);
    }
    if !valid_host(&new_place.host) {
        return Err(RedirectRefusal::Host);
    }
    let Some(scope) = new_place.scope else {
        return Err(RedirectRefusal::NoScope);
    };
    if new_place.port == Some(0) {
        return Err(RedirectRefusal::Host);
    }
    if visited
        .iter()
        .any(|(h, s)| *h == new_place.host && *s == scope)
    {
        return Err(RedirectRefusal::Visited);
    }
    Ok(())
}

// ----------------------------------------------------------------------------------------------- base32 / base64

fn base32(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 8 / 5 + 1);
    let (mut acc, mut bits) = (0u32, 0u32);
    for x in b {
        acc = acc << 8 | *x as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[(acc >> bits & 31) as usize] as char);
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(ALPHABET[(acc << (5 - bits) & 31) as usize] as char);
    }
    out
}

/// Strict: upper-case alphabet only, and the unused trailing bits must be zero.
fn unbase32(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 5 / 8);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = ALPHABET.iter().position(|a| *a == c)? as u32;
        acc = acc << 5 | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (bits < 5 && acc == 0 && !out.is_empty()).then_some(out)
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len() * 4 / 3 + 2);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn unb64url(s: &str) -> Option<Vec<u8>> {
    if s.len() > MAX_TEXT_CHARS || s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = B64.iter().position(|a| *a == c)? as u32;
        acc = acc << 6 | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    (acc == 0 && !out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full() -> Sheet {
        Sheet::full([7; 32], [9; 32], None, "home.example.net").unwrap()
    }

    #[test]
    fn round_trips_in_every_form() {
        for s in [
            full(),
            full().place(),
            Sheet::full([1; 32], [2; 32], Some(8443), "a.bc").unwrap(),
        ] {
            let kind = s.is_full();
            for text in [s.to_compact(), s.to_sheet_form(), s.to_qr()] {
                match parse(&text).unwrap() {
                    Parsed::Sheet(p) => {
                        assert_eq!(p, s);
                        assert_eq!(p.is_full(), kind);
                    }
                    other => panic!("{other:?}"),
                }
            }
            // case, spaces, hyphens and line breaks are ignored in the compact form
            let messy = s.to_compact().to_lowercase().replace("", " ");
            assert_eq!(parse(&messy).unwrap(), Parsed::Sheet(s));
        }
    }

    #[test]
    fn sizes_match_the_spec() {
        assert_eq!(full().to_bytes().len(), 73 + "home.example.net".len());
        assert_eq!(
            full().place().to_bytes().len(),
            41 + "home.example.net".len()
        );
    }

    #[test]
    fn typed_line_feedback_names_the_line() {
        let s = Sheet::full([3; 32], [4; 32], None, "home.example.net").unwrap();
        let form = s.to_sheet_form();
        assert!(form.lines().count() >= 4);
        assert_eq!(first_bad_line(&form), None);
        // change one character on line 2 (keep it in the alphabet)
        let mut lines: Vec<String> = form.lines().map(str::to_string).collect();
        let c = lines[1].chars().next().unwrap();
        let swap = if c == 'A' { 'B' } else { 'A' };
        lines[1].replace_range(0..1, &swap.to_string());
        let bad = lines.join("\n");
        assert_eq!(first_bad_line(&bad), Some(2));
        assert_eq!(parse(&bad), Err(SheetError::LineCheck(2)));
    }

    #[test]
    fn legacy_key_only_text_is_accepted_for_entry() {
        let root = [5u8; 32];
        let chk = crate::crypto::sha256(&[b"OM-RECOVERY-KEY-DISPLAY-CHECK", &root]);
        let mut raw = root.to_vec();
        raw.extend_from_slice(&chk[..3]);
        let text = base32(&raw);
        assert_eq!(text.len(), 56);
        assert_eq!(parse(&text), Ok(Parsed::Legacy(LegacyKey(root))));
        let mut bad = raw.clone();
        bad[34] ^= 1;
        assert_eq!(parse(&base32(&bad)), Err(SheetError::Check));
    }

    #[test]
    fn one_confusable_correction_only_when_unique() {
        let s = full();
        let good = s.to_compact();
        // put a 0 where an O is, if the string has one; otherwise a 1 where an I or L is
        let Some(i) = good.find(['O', 'I', 'L', 'B']) else {
            return;
        };
        let swapped = match &good[i..=i] {
            "O" => "0",
            "B" => "8",
            _ => "1",
        };
        let typed = format!("{}{swapped}{}", &good[..i], &good[i + 1..]);
        assert_eq!(parse(&typed), Err(SheetError::Alphabet));
        assert_eq!(suggest_correction(&typed), Some(good.clone()));
        // two confusables: still one substitution at a time cannot make it pass
        let two = typed.replacen(['O', 'I', 'L', 'B'], "0", 1);
        if two != typed {
            assert_eq!(suggest_correction(&two), None);
        }
    }

    #[test]
    fn redirect_rules() {
        use crate::recovery_record::ProviderHome;
        let ok = ProviderHome::new("new.example.org", None, Some([1; 32]));
        assert_eq!(check_redirect(&ok, &[], 0), Ok(()));
        assert_eq!(
            check_redirect(&ok, &[], 3),
            Err(RedirectRefusal::TooManyHops)
        );
        assert_eq!(
            check_redirect(&ok, &[("new.example.org".into(), [1; 32])], 1),
            Err(RedirectRefusal::Visited)
        );
        assert_eq!(
            check_redirect(&ok, &[("new.example.org".into(), [2; 32])], 1),
            Ok(()),
            "same host, another scope is not a loop"
        );
        assert_eq!(
            check_redirect(
                &ProviderHome::new("https://x.org", None, Some([1; 32])),
                &[],
                0
            ),
            Err(RedirectRefusal::Host)
        );
        assert_eq!(
            check_redirect(&ProviderHome::new("127.0.0.1", None, Some([1; 32])), &[], 0),
            Err(RedirectRefusal::Host)
        );
        assert_eq!(
            check_redirect(&ProviderHome::new("x.example.org", None, None), &[], 0),
            Err(RedirectRefusal::NoScope)
        );
    }
}
