// SPDX-License-Identifier: MPL-2.0
//! Human-readable export (portability slice 5; IMPORT_EXPORT_AND_USER_ESCAPE.md "Disappearing and ViewOnce content" rules
//! 1 and 4, ADR-0129, ADR-0263). Plain text files for people to read; they are plaintext on disk and not the migration
//! format.
//!
//! * The plaintext warning travels with every export and is the first line of each file.
//! * ViewOnce items are never written. Disappearing items are excluded unless the export explicitly chose to include them,
//!   and that choice is recorded in the result (a missing choice means excluded and is recorded as such).
//! * Text is neutralized: control and bidirectional-override characters never reach the file.

pub const PLAINTEXT_WARNING: &str = "This file is readable plain text. Anyone who can open it can read it, and items that were set to disappear will not disappear from it. It is a copy to read, not a way to move to another device or app.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub time: String,
    pub author: String,
    pub text: String,
    pub disappearing: bool,
    pub view_once: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisappearingChoice {
    Excluded,
    /// The person chose to include them, after seeing the warning that the files will not disappear.
    IncludedExplicitly,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReadableExport {
    /// `(path, bytes)`.
    pub files: Vec<(String, Vec<u8>)>,
    pub disappearing_choice: DisappearingChoice,
    pub excluded_disappearing: usize,
    pub excluded_view_once: usize,
    /// Always false: this is never the migration format.
    pub is_migration_format: bool,
    pub warning: &'static str,
}

/// Replaces control characters (except newline within a message) and bidi overrides, so a message cannot reorder or hide
/// text in the file.
pub fn neutralize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' => '\n',
            c if c.is_control() => '\u{FFFD}',
            '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{061C}' => '\u{FFFD}',
            c => c,
        })
        .collect()
}

/// A safe file name for a conversation title.
pub fn file_name(title: &str, index: usize) -> String {
    let base: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .take(40)
        .collect();
    let base = base.trim().replace(' ', "_");
    if base.is_empty() {
        format!("conversation-{index}.txt")
    } else {
        format!("{base}-{index}.txt")
    }
}

/// Renders conversations `(title, messages)`. `include_disappearing` is the person's explicit choice, if any.
pub fn export(
    conversations: &[(String, Vec<Message>)],
    include_disappearing: Option<bool>,
) -> ReadableExport {
    let include = include_disappearing == Some(true);
    let mut files = Vec::new();
    let (mut ex_dis, mut ex_once) = (0, 0);
    for (i, (title, msgs)) in conversations.iter().enumerate() {
        let mut out = format!("{PLAINTEXT_WARNING}\n\n{}\n\n", neutralize(title));
        for m in msgs {
            if m.view_once {
                ex_once += 1;
                continue;
            }
            if m.disappearing && !include {
                ex_dis += 1;
                continue;
            }
            out.push_str(&format!(
                "[{}] {}: {}\n",
                neutralize(&m.time),
                neutralize(&m.author),
                neutralize(&m.text)
            ));
        }
        files.push((file_name(title, i + 1), out.into_bytes()));
    }
    ReadableExport {
        files,
        disappearing_choice: if include {
            DisappearingChoice::IncludedExplicitly
        } else {
            DisappearingChoice::Excluded
        },
        excluded_disappearing: ex_dis,
        excluded_view_once: ex_once,
        is_migration_format: false,
        warning: PLAINTEXT_WARNING,
    }
}
