// SPDX-License-Identifier: MPL-2.0
//! Foreign import framework (portability slice 6; IMPORT_EXPORT_AND_USER_ESCAPE.md section 11, ADR-0263). An adapter turns
//! bytes from another messenger into local historical records. It runs where it has no keys and no network (the host
//! provides that sandbox; this module hands an adapter nothing but the input bytes and the limits).
//!
//! * [`ImportAdapter`] is the contract: `detect`, `parse_bounded`, `normalize`, `enumerate_blobs`, `emit_warnings`.
//! * [`run`] enforces the bounds, neutralizes text and always produces records at the lowest provenance level. A
//!   [`Draft`] has no field for an OM EventID, author, conversation or signature, so no record can gain one.
//! * The [`PortabilityReport`] states how many records were carried, refused and degraded, and carries the adapter's
//!   warnings.

use crate::readable::neutralize;

pub const LEVEL: &str = "foreign_export_label";

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub max_input_bytes: usize,
    pub max_records: usize,
    pub max_text_bytes: usize,
    pub max_blobs: usize,
}

impl Bounds {
    pub const DEFAULT: Bounds = Bounds {
        max_input_bytes: 64 * 1024 * 1024,
        max_records: 1_000_000,
        max_text_bytes: 64 * 1024,
        max_blobs: 100_000,
    };
}

/// What an adapter yields: exactly the fields of an imported record, none of the native ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub source_network_namespace: String,
    pub source_conversation_ref: Option<String>,
    pub source_event_ref: Option<String>,
    pub source_actor_ref: Option<String>,
    pub display_name: Option<String>,
    pub text: String,
    pub blob_refs: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AdapterError {
    /// The adapter does not recognize the input.
    NotRecognized,
    /// Input is malformed or exceeds a bound.
    Rejected(&'static str),
}

pub trait ImportAdapter {
    fn namespace(&self) -> &str;
    fn detect(&self, head: &[u8]) -> bool;
    /// Parses with the given bounds; must not recurse or allocate beyond them.
    fn parse_bounded(&self, input: &[u8], bounds: &Bounds) -> Result<Vec<Draft>, AdapterError>;
    fn enumerate_blobs(&self, drafts: &[Draft]) -> Vec<String>;
    fn emit_warnings(&self) -> Vec<String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub imported_record_id: String,
    pub draft: Draft,
    /// Always [`LEVEL`]: an adapter never upgrades provenance.
    pub provenance_level: &'static str,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PortabilityReport {
    pub carried: usize,
    /// Records whose text was truncated or neutralized.
    pub degraded: usize,
    pub refused: usize,
    pub blobs: usize,
    pub warnings: Vec<String>,
}

pub fn run(
    a: &dyn ImportAdapter,
    input: &[u8],
    b: &Bounds,
) -> Result<(Vec<Record>, PortabilityReport), AdapterError> {
    if input.len() > b.max_input_bytes {
        return Err(AdapterError::Rejected("input too large"));
    }
    if !a.detect(&input[..input.len().min(4096)]) {
        return Err(AdapterError::NotRecognized);
    }
    let drafts = a.parse_bounded(input, b)?;
    if drafts.len() > b.max_records {
        return Err(AdapterError::Rejected("too many records"));
    }
    let blobs = a.enumerate_blobs(&drafts);
    if blobs.len() > b.max_blobs {
        return Err(AdapterError::Rejected("too many blobs"));
    }
    let mut report = PortabilityReport {
        blobs: blobs.len(),
        warnings: a.emit_warnings(),
        ..Default::default()
    };
    let mut out = Vec::with_capacity(drafts.len());
    for (i, mut d) in drafts.into_iter().enumerate() {
        let clean = neutralize(&d.text);
        let mut degraded = clean != d.text;
        d.text = clean;
        if d.text.len() > b.max_text_bytes {
            let mut cut = b.max_text_bytes;
            while !d.text.is_char_boundary(cut) {
                cut -= 1;
            }
            d.text.truncate(cut);
            degraded = true;
        }
        if d.text.is_empty() && d.blob_refs.is_empty() {
            report.refused += 1;
            continue;
        }
        d.display_name = d.display_name.map(|n| neutralize(&n));
        if degraded {
            report.degraded += 1;
        }
        report.carried += 1;
        out.push(Record {
            imported_record_id: format!("{}-{}", a.namespace(), i + 1),
            draft: d,
            provenance_level: LEVEL,
        });
    }
    Ok((out, report))
}
