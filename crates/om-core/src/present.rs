// SPDX-License-Identifier: MPL-2.0
//! V1 presentation extensions of a `message.create` (ADR-0148, ADR-0149, ADR-0155): `om.voice-note.v1` and
//! `om.location-card.v1`. Both are non-critical key-23 entries: they only change how an ordinary W0 message is drawn.
//! A value that fails its schema, or whose binding to the message fails, is *ignored* and the plain W0 message renders
//! unchanged; it never rejects the event and never reinterprets the attachment or the text (ADR-0155 section 1).
//!
//! Nothing in them is trusted: the waveform is a bounded hint, the label is sender-authored text, and the point is
//! shown only while the message's currently materialized text carries exactly the canonical `geo:` token of that point.

use crate::crypto::cbor::V;

pub const EXT_VOICE: &str = "om.voice-note.v1";
pub const EXT_LOCATION: &str = "om.location-card.v1";

pub const WAVEFORM_MIN: usize = 16;
pub const WAVEFORM_MAX: usize = 256;
pub const LABEL_MAX_BYTES: usize = 256;
pub const ACCURACY_MAX_M: u32 = 1_000_000;
const LAT_MAX_E7: i64 = 900_000_000;
const LON_MAX_E7: i64 = 1_800_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voice {
    pub attachment_index: u8,
    pub waveform: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub lat_e7: i32,
    pub lon_e7: i32,
    pub accuracy_m: Option<u32>,
    pub label: Option<String>,
}

/// The presentation extensions a create carried and that passed their schema (binding checks happen at view time).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    pub voice: Option<Voice>,
    pub location: Option<Location>,
}

// ---------------------------------------------------------------------------------------------- encoding

/// A non-critical extension entry `{0: false, 1: value}`.
fn entry(value: V) -> V {
    V::M(vec![(0, V::Bool(false)), (1, value)])
}

impl Voice {
    pub fn value(&self) -> V {
        let mut m = vec![(0, V::U(self.attachment_index as u64))];
        if let Some(w) = &self.waveform {
            m.push((1, V::B(w.clone())));
        }
        V::M(m)
    }
}

impl Location {
    pub fn value(&self) -> V {
        let mut m = vec![
            (0, V::U((self.lat_e7 as i64 + LAT_MAX_E7) as u64)),
            (1, V::U((self.lon_e7 as i64 + LON_MAX_E7) as u64)),
        ];
        if let Some(a) = self.accuracy_m {
            m.push((2, V::U(a as u64)));
        }
        if let Some(l) = &self.label {
            m.push((3, V::T(l.clone())));
        }
        V::M(m)
    }
}

/// The key-23 map for the extensions of a create, or `None` when there are none (key 23 is never empty).
pub fn extensions_value(ext: &Extensions) -> Option<V> {
    let mut m = vec![];
    if let Some(l) = &ext.location {
        m.push((EXT_LOCATION.to_string(), entry(l.value())));
    }
    if let Some(v) = &ext.voice {
        m.push((EXT_VOICE.to_string(), entry(v.value())));
    }
    (!m.is_empty()).then_some(V::TM(m))
}

fn parse_voice(v: &V) -> Option<Voice> {
    let idx = v.get(0).ok()?.u().ok()?;
    if idx > 31 {
        return None;
    }
    let waveform = match v.opt(1) {
        None => None,
        Some(w) => {
            let w = w.b().ok()?;
            if !(WAVEFORM_MIN..=WAVEFORM_MAX).contains(&w.len()) {
                return None;
            }
            Some(w.to_vec())
        }
    };
    Some(Voice {
        attachment_index: idx as u8,
        waveform,
    })
}

fn parse_location(v: &V) -> Option<Location> {
    let lat = v.get(0).ok()?.u().ok()?;
    let lon = v.get(1).ok()?.u().ok()?;
    if lat > 2 * LAT_MAX_E7 as u64 || lon > 2 * LON_MAX_E7 as u64 {
        return None;
    }
    let accuracy_m = match v.opt(2) {
        None => None,
        Some(a) => {
            let a = a.u().ok()?;
            if !(1..=ACCURACY_MAX_M as u64).contains(&a) {
                return None;
            }
            Some(a as u32)
        }
    };
    let label = match v.opt(3) {
        None => None,
        Some(l) => {
            let l = l.t().ok()?;
            if l.is_empty() || l.len() > LABEL_MAX_BYTES {
                return None;
            }
            Some(crate::flows::sanitize_label(l))
        }
    };
    Some(Location {
        lat_e7: (lat as i64 - LAT_MAX_E7) as i32,
        lon_e7: (lon as i64 - LON_MAX_E7) as i32,
        accuracy_m,
        label: label.filter(|l| !l.is_empty()),
    })
}

/// Read the key-23 map of a create. Registered extensions that fail their schema are dropped silently; an entry whose
/// wire criticality is not the registered `false` makes the whole event invalid (`None`), as the W0 registry check already
/// enforces before this runs. Unknown non-critical extensions are ignored (ADR-0155).
pub fn parse_extensions(key23: Option<&V>) -> Option<Extensions> {
    let mut out = Extensions::default();
    let Some(m) = key23 else {
        return Some(out);
    };
    for (id, e) in m.tm().ok()? {
        let critical = match e.get(0).ok()? {
            V::Bool(b) => *b,
            _ => return None,
        };
        let value = e.get(1).ok()?;
        match id.as_str() {
            EXT_VOICE | EXT_LOCATION if critical => return None,
            EXT_VOICE => out.voice = parse_voice(value),
            EXT_LOCATION => out.location = parse_location(value),
            _ if critical => return None, // an unknown critical extension is never half-applied
            _ => {}
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------- geo token

/// Signed E7 as base-10 degrees with exactly seven fraction digits, no `+`, `-` only for negatives (ADR-0149 section 2).
pub fn fixed7(e7: i64) -> String {
    let sign = if e7 < 0 { "-" } else { "" };
    let a = e7.unsigned_abs();
    format!("{sign}{}.{:07}", a / 10_000_000, a % 10_000_000)
}

/// The canonical token `geo:<lat>,<lon>` of a point.
pub fn geo_token(lat_e7: i32, lon_e7: i32) -> String {
    format!("geo:{},{}", fixed7(lat_e7 as i64), fixed7(lon_e7 as i64))
}

/// Parse `-?\d{1,3}\.\d{7}` at the start of `s`; return the value in E7 and the bytes consumed. Canonical only: the value
/// must render back to exactly the consumed text (no leading zeros, no `-0.0000000`, no `+`).
fn coordinate(s: &[u8]) -> Option<(i64, usize)> {
    let mut i = 0;
    let neg = s.first() == Some(&b'-');
    if neg {
        i += 1;
    }
    let int_start = i;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    let int_len = i - int_start;
    if !(1..=3).contains(&int_len) || s.get(i) != Some(&b'.') {
        return None;
    }
    let int: i64 = core::str::from_utf8(&s[int_start..i]).ok()?.parse().ok()?;
    i += 1;
    let f_start = i;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
    }
    if i - f_start != 7 {
        return None;
    }
    let frac: i64 = core::str::from_utf8(&s[f_start..i]).ok()?.parse().ok()?;
    let mag = int * 10_000_000 + frac;
    let v = if neg { -mag } else { mag };
    (fixed7(v).as_bytes() == &s[..i]).then_some((v, i))
}

fn token_at(b: &[u8], at: usize) -> Option<(i32, i32)> {
    // Boundary before: `geo:` must not continue a word (`xgeo:` is ordinary text).
    if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
        return None;
    }
    let rest = &b[at + 4..];
    let (lat, n1) = coordinate(rest)?;
    if rest.get(n1) != Some(&b',') {
        return None;
    }
    let (lon, n2) = coordinate(&rest[n1 + 1..])?;
    if lat.abs() > LAT_MAX_E7 || lon.abs() > LON_MAX_E7 {
        return None;
    }
    // Boundary after: parameters, more digits or a word continuing the token make it a different, non-canonical URI.
    let end = n1 + 1 + n2;
    match rest.get(end) {
        None => {}
        Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'_' | b';' | b'?' | b'-') => {
            return None;
        }
        Some(b'.' | b',') if rest.get(end + 1).is_some_and(|d| d.is_ascii_digit()) => return None,
        Some(_) => {}
    }
    Some((lat as i32, lon as i32))
}

/// Every distinct canonical point named in `text` (ADR-0155 section 3: a fallback must name exactly one).
pub fn canonical_points(text: &str) -> Vec<(i32, i32)> {
    let b = text.as_bytes();
    let mut out: Vec<(i32, i32)> = vec![];
    let mut i = 0;
    while i + 4 <= b.len() {
        if &b[i..i + 4] == b"geo:"
            && let Some(p) = token_at(b, i)
            && !out.contains(&p)
        {
            out.push(p);
        }
        i += 1;
    }
    out
}

/// Does `text` carry the canonical token of exactly this point and of no other point?
pub fn fallback_names_exactly(text: &str, lat_e7: i32, lon_e7: i32) -> bool {
    canonical_points(text) == [(lat_e7, lon_e7)]
}

// ---------------------------------------------------------------------------------------------- bindings

/// A voice note is meaningful only on an audio attachment that carries a duration (ADR-0148 section 3).
pub fn voice_binds(v: &Voice, attachments: &[crate::attach::AttachmentRef]) -> bool {
    attachments
        .get(v.attachment_index as usize)
        .is_some_and(|a| a.media_type.starts_with("audio/") && a.duration_ms.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_rendering_and_parsing_follow_adr_0149() {
        assert_eq!(geo_token(0, 0), "geo:0.0000000,0.0000000");
        assert_eq!(
            geo_token(107_202_000, 1_225_621_000),
            "geo:10.7202000,122.5621000"
        );
        assert_eq!(
            geo_token(-338_688_000, 1_512_093_000),
            "geo:-33.8688000,151.2093000"
        );
        assert_eq!(
            geo_token(-900_000_000, -1_800_000_000),
            "geo:-90.0000000,-180.0000000"
        );
        assert!(fallback_names_exactly(
            "here: geo:10.7202000,122.5621000.",
            107_202_000,
            1_225_621_000
        ));
        assert!(fallback_names_exactly(
            "geo:0.0000000,0.0000000 and again geo:0.0000000,0.0000000",
            0,
            0
        ));
    }

    #[test]
    fn non_canonical_or_ambiguous_fallbacks_do_not_count() {
        let p = (107_202_000, 1_225_621_000);
        let ok = |t: &str| fallback_names_exactly(t, p.0, p.1);
        assert!(!ok("no token"));
        assert!(!ok("geo:10.72020,122.56210"), "wrong precision");
        assert!(!ok("geo:10.7202000,122.5621000;u=30"), "parameters");
        assert!(!ok("geo:10.7202000,122.5621000,5"), "altitude");
        assert!(!ok("xgeo:10.7202000,122.5621000"), "word boundary before");
        assert!(!ok("geo:10.7202000,122.56210001"), "more digits");
        assert!(!ok("geo:+10.7202000,122.5621000"));
        assert!(!ok("geo:010.7202000,122.5621000"), "leading zero");
        assert!(!ok("geo:-0.0000000,0.0000000"));
        assert!(!ok("geo:90.0000001,0.0000000"), "out of range");
        // a second DIFFERENT canonical point makes the card unusable; the same point twice is fine.
        assert!(!ok("geo:10.7202000,122.5621000 or geo:0.0000000,0.0000000"));
        assert!(ok("geo:10.7202000,122.5621000 geo:10.7202000,122.5621000"));
        // a non-canonical second URI is ordinary text.
        assert!(ok("geo:10.7202000,122.5621000 or geo:1.5,2.5"));
    }

    #[test]
    fn extensions_round_trip_and_bad_values_are_ignored() {
        let ext = Extensions {
            voice: Some(Voice {
                attachment_index: 0,
                waveform: Some(vec![7; 32]),
            }),
            location: Some(Location {
                lat_e7: -338_688_000,
                lon_e7: 1_512_093_000,
                accuracy_m: Some(25),
                label: Some("Harbour".into()),
            }),
        };
        let v = extensions_value(&ext).unwrap();
        let bytes = crate::crypto::cbor::encode(&v);
        let back = crate::crypto::cbor::decode(&bytes).unwrap();
        assert_eq!(parse_extensions(Some(&back)).unwrap(), ext);

        // schema failures: a 3-sample waveform, an index of 32, coordinates out of range -> ignored, event still valid
        let bad = V::TM(vec![
            (
                EXT_VOICE.into(),
                entry(V::M(vec![(0, V::U(0)), (1, V::B(vec![1; 3]))])),
            ),
            (
                EXT_LOCATION.into(),
                entry(V::M(vec![(0, V::U(2_000_000_000)), (1, V::U(0))])),
            ),
        ]);
        assert_eq!(parse_extensions(Some(&bad)).unwrap(), Extensions::default());
        // criticality is registry-bound: a critical om.voice-note.v1 is not a presentation hint
        let crit = V::TM(vec![(
            EXT_VOICE.into(),
            V::M(vec![(0, V::Bool(true)), (1, V::M(vec![(0, V::U(0))]))]),
        )]);
        assert!(parse_extensions(Some(&crit)).is_none());
        // unknown critical extension fails closed; unknown non-critical is ignored
        let unk = |c| {
            V::TM(vec![(
                "om.future.v9".into(),
                V::M(vec![(0, V::Bool(c)), (1, V::U(1))]),
            )])
        };
        assert!(parse_extensions(Some(&unk(true))).is_none());
        assert_eq!(
            parse_extensions(Some(&unk(false))).unwrap(),
            Extensions::default()
        );
    }
}
