//! Integration tests for ClipboardHistory with real NSPasteboard.
//!
//! These tests modify the system clipboard. Run serially:
//!   cargo test --test clipboard_integration -- --test-threads=1

use cliphop::clipboard::ClipboardHistory;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSString, ns_string};

/// Write a string to the system clipboard.
fn write_to_clipboard(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let ns_text = NSString::from_str(text);
    pasteboard.setString_forType(&ns_text, ns_string!("public.utf8-plain-text"));
}

/// Read the current string from the system clipboard.
fn read_from_clipboard() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard
        .stringForType(ns_string!("public.utf8-plain-text"))
        .map(|s| s.to_string())
}

// ── poll() tests ─────────────────────────────────────────────────────

#[test]
fn poll_detects_new_clipboard_content() {
    let mut history = ClipboardHistory::new();
    write_to_clipboard("test_poll_1");

    assert!(
        history.poll().is_some(),
        "poll() should return true for new content"
    );
    assert_eq!(history.items().len(), 1);
    assert_eq!(history.items()[0], "test_poll_1");
}

#[test]
fn poll_returns_false_when_no_change() {
    let mut history = ClipboardHistory::new();
    write_to_clipboard("test_no_change");
    history.poll();

    assert!(
        history.poll().is_none(),
        "poll() should return false when unchanged"
    );
}

#[test]
fn poll_ignores_empty_clipboard() {
    let mut history = ClipboardHistory::new();
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    let ns_text = NSString::from_str("");
    pasteboard.setString_forType(&ns_text, ns_string!("public.utf8-plain-text"));

    assert!(
        history.poll().is_none(),
        "poll() should return false for empty string"
    );
    assert_eq!(history.items().len(), 0);
}

// ── Deduplication ────────────────────────────────────────────────────

#[test]
fn poll_deduplicates_moves_to_front() {
    let mut history = ClipboardHistory::new();

    write_to_clipboard("item_A");
    history.poll();
    write_to_clipboard("item_B");
    history.poll();

    assert_eq!(history.items()[0], "item_B");
    assert_eq!(history.items()[1], "item_A");

    // Re-copy item_A — should move to front, not create duplicate
    write_to_clipboard("item_A");
    history.poll();

    assert_eq!(history.items().len(), 2);
    assert_eq!(history.items()[0], "item_A");
    assert_eq!(history.items()[1], "item_B");
}

// ── MAX_HISTORY cap ──────────────────────────────────────────────────

#[test]
fn poll_caps_at_max_history() {
    let mut history = ClipboardHistory::new();

    for i in 0..12 {
        write_to_clipboard(&format!("overflow_{}", i));
        history.poll();
    }

    assert_eq!(history.items().len(), 10);
    assert_eq!(history.items()[0], "overflow_11");
    assert_eq!(history.items()[9], "overflow_2");
}

// ── select() tests ───────────────────────────────────────────────────

#[test]
fn select_writes_to_clipboard() {
    cliphop::log::init();
    let mut history = ClipboardHistory::new();

    write_to_clipboard("sel_A");
    history.poll();
    write_to_clipboard("sel_B");
    history.poll();

    let result = history.select(1);
    assert_eq!(result, Some("sel_A".to_string()));
    assert_eq!(read_from_clipboard(), Some("sel_A".to_string()));
}

#[test]
fn select_out_of_bounds_returns_none() {
    cliphop::log::init();
    let mut history = ClipboardHistory::new();

    write_to_clipboard("only_item");
    history.poll();

    assert_eq!(history.select(5), None);
    assert_eq!(history.select(1), None);
}

#[test]
fn select_does_not_trigger_duplicate_on_next_poll() {
    cliphop::log::init();
    let mut history = ClipboardHistory::new();

    write_to_clipboard("no_dup_A");
    history.poll();
    write_to_clipboard("no_dup_B");
    history.poll();

    history.select(1); // writes "no_dup_A" to clipboard

    assert!(
        history.poll().is_none(),
        "poll() after select() should not detect a change"
    );
    assert_eq!(history.items().len(), 2);
}

// ── E2E sanity flow ──────────────────────────────────────────────────

#[test]
fn e2e_copy_paste_flow() {
    cliphop::log::init();
    let mut history = ClipboardHistory::new();

    // 1. Copy "item A" → poll → verify
    write_to_clipboard("item A");
    assert!(history.poll().is_some());
    assert_eq!(history.items().len(), 1);
    assert_eq!(history.items()[0], "item A");

    // 2. Copy "item B" → poll → verify ordering
    write_to_clipboard("item B");
    assert!(history.poll().is_some());
    assert_eq!(history.items().len(), 2);
    assert_eq!(history.items()[0], "item B");
    assert_eq!(history.items()[1], "item A");

    // 3. Re-copy "item A" → poll → verify deduplication
    write_to_clipboard("item A");
    assert!(history.poll().is_some());
    assert_eq!(history.items().len(), 2);
    assert_eq!(history.items()[0], "item A");
    assert_eq!(history.items()[1], "item B");

    // 4. Select index 1 ("item B") → verify clipboard updated
    let selected = history.select(1);
    assert_eq!(selected, Some("item B".to_string()));
    assert_eq!(read_from_clipboard(), Some("item B".to_string()));

    // 5. Fill to MAX_HISTORY → verify cap
    for i in 0..10 {
        write_to_clipboard(&format!("fill_{}", i));
        history.poll();
    }
    assert_eq!(history.items().len(), 10);
    assert_eq!(history.items()[0], "fill_9");

    // 6. Select index 0 → verify clipboard
    let top = history.select(0);
    assert_eq!(top, Some("fill_9".to_string()));
    assert_eq!(read_from_clipboard(), Some("fill_9".to_string()));

    // 7. Verify poll() does not re-detect the select-write
    assert!(history.poll().is_none());
}
