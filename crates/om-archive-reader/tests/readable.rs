// SPDX-License-Identifier: MPL-2.0
//! Portability slice 5: warning on every file, ViewOnce never written, disappearing items excluded unless explicitly
//! chosen and the choice recorded, text neutralized, never the migration format.
use om_archive_reader::readable::*;

fn m(text: &str, disappearing: bool, view_once: bool) -> Message {
    Message {
        time: "2026-11-24 10:00".into(),
        author: "Ana".into(),
        text: text.into(),
        disappearing,
        view_once,
    }
}
fn convo() -> Vec<(String, Vec<Message>)> {
    vec![(
        "Family chat".into(),
        vec![
            m("hello", false, false),
            m("secret", true, false),
            m("once", false, true),
        ],
    )]
}
fn text(e: &ReadableExport) -> String {
    String::from_utf8(e.files[0].1.clone()).unwrap()
}

#[test]
fn the_default_excludes_disappearing_items_and_never_writes_view_once() {
    let e = export(&convo(), None);
    let t = text(&e);
    assert!(t.starts_with(PLAINTEXT_WARNING));
    assert!(t.contains("hello") && !t.contains("secret") && !t.contains("once"));
    assert_eq!((e.excluded_disappearing, e.excluded_view_once), (1, 1));
    assert_eq!(e.disappearing_choice, DisappearingChoice::Excluded);
    assert!(!e.is_migration_format);
    assert_eq!(e.warning, PLAINTEXT_WARNING);
}

#[test]
fn including_disappearing_items_is_an_explicit_recorded_choice_and_view_once_stays_out() {
    let e = export(&convo(), Some(true));
    let t = text(&e);
    assert!(t.contains("secret") && !t.contains("once"));
    assert_eq!(
        e.disappearing_choice,
        DisappearingChoice::IncludedExplicitly
    );
    assert_eq!((e.excluded_disappearing, e.excluded_view_once), (0, 1));
    assert_eq!(
        export(&convo(), Some(false)).disappearing_choice,
        DisappearingChoice::Excluded
    );
}

#[test]
fn control_and_bidi_characters_never_reach_the_file() {
    let c = vec![(
        "t\u{202E}itle".to_string(),
        vec![m("a\u{202E}b\u{0007}c\nline", false, false)],
    )];
    let t = String::from_utf8(export(&c, None).files[0].1.clone()).unwrap();
    assert!(!t.contains('\u{202E}') && !t.contains('\u{0007}'));
    assert!(t.contains("a\u{FFFD}b\u{FFFD}c\nline"));
}

#[test]
fn file_names_are_safe_and_never_path_like() {
    assert_eq!(file_name("../../etc/passwd", 1), "etcpasswd-1.txt");
    assert_eq!(file_name("???", 2), "conversation-2.txt");
    assert_eq!(file_name("Family chat", 3), "Family_chat-3.txt");
}
