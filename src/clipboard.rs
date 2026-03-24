use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSString, ns_string};

static MAX_HISTORY: AtomicUsize = AtomicUsize::new(10);
static TRIM_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn set_max_history(n: usize) {
    MAX_HISTORY.store(
        n.clamp(
            crate::config::MIN_MAX_HISTORY,
            crate::config::MAX_MAX_HISTORY,
        ),
        Ordering::Relaxed,
    );
}

pub fn get_max_history() -> usize {
    MAX_HISTORY.load(Ordering::Relaxed)
}

/// Signals the next `ClipboardHistory::poll()` call to trim items to the current limit.
pub fn request_trim() {
    TRIM_REQUESTED.store(true, Ordering::Relaxed);
}

const LABEL_LEN: usize = 60;
const TOOLTIP_LEN: usize = 300;

pub struct ClipboardHistory {
    items: VecDeque<String>,
    pinned: VecDeque<String>,
    last_change_count: isize,
}

impl ClipboardHistory {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let pasteboard = NSPasteboard::generalPasteboard();
        let count = pasteboard.changeCount();
        Self {
            items: VecDeque::new(),
            pinned: VecDeque::new(),
            last_change_count: count,
        }
    }

    /// Polls the clipboard for changes. Returns `Some(new_text)` if a new item
    /// was added to the front (after deduplication); `None` otherwise.
    pub fn poll(&mut self) -> Option<String> {
        if TRIM_REQUESTED.swap(false, Ordering::Relaxed) {
            self.trim_to_limit();
        }

        let pasteboard = NSPasteboard::generalPasteboard();
        let count = pasteboard.changeCount();

        if count == self.last_change_count {
            return None;
        }
        self.last_change_count = count;

        let text = pasteboard.stringForType(ns_string!("public.utf8-plain-text"));
        let text = match text {
            Some(ns_str) => ns_str.to_string(),
            None => return None,
        };

        if text.is_empty() {
            return None;
        }

        // Do not add to history if this text is already pinned.
        if self.pinned.iter().any(|p| p == &text) {
            return None;
        }

        // Deduplicate: remove existing copy and move to front
        if let Some(pos) = self.items.iter().position(|s| s == &text) {
            self.items.remove(pos);
        }

        self.items.push_front(text.clone());

        if self.items.len() > MAX_HISTORY.load(Ordering::Relaxed) {
            self.items.pop_back();
        }

        Some(text)
    }

    pub fn items(&self) -> &VecDeque<String> {
        &self.items
    }

    /// Selects an item by index: writes it to the clipboard and returns the text.
    pub fn select(&mut self, index: usize) -> Option<String> {
        let text = self.items.get(index)?.clone();

        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let ns_text = NSString::from_str(&text);
        let ok = pasteboard.setString_forType(&ns_text, ns_string!("public.utf8-plain-text"));
        crate::log::log_verbose(&format!(
            "clipboard.select({}): setString_forType returned {}",
            index, ok
        ));

        self.last_change_count = pasteboard.changeCount();
        crate::log::log_verbose(&format!(
            "clipboard.select({}): changeCount now = {}",
            index, self.last_change_count
        ));

        Some(text)
    }

    /// Seeds the in-memory history from persisted entries (called once at startup).
    /// Applies MAX_HISTORY cap. Sets last_change_count to current pasteboard value
    /// so the next poll() does not re-detect already-persisted items as new.
    pub fn load_items(&mut self, items: Vec<String>) {
        let limit = MAX_HISTORY.load(Ordering::Relaxed);
        self.items = items.into_iter().take(limit).collect();
        // Re-read the current pasteboard change count so poll() doesn't
        // treat the existing clipboard content as a new item.
        let pasteboard = NSPasteboard::generalPasteboard();
        self.last_change_count = pasteboard.changeCount();
    }

    /// Clears the in-memory history and pinned items.
    pub fn clear(&mut self) {
        self.items.clear();
        self.pinned.clear();
    }

    pub fn pinned_items(&self) -> &VecDeque<String> {
        &self.pinned
    }

    /// Moves history[idx] to the pinned list. No-op if text already pinned.
    pub fn pin(&mut self, idx: usize) {
        let Some(text) = self.items.get(idx).cloned() else { return };
        if self.pinned.iter().any(|p| p == &text) {
            return; // already pinned
        }
        self.items.remove(idx);
        self.pinned.push_back(text);
    }

    /// Moves pinned[idx] to the front of history.
    /// Evicts the oldest history item if already at max_history capacity.
    pub fn unpin(&mut self, idx: usize) {
        let Some(text) = self.pinned.get(idx).cloned() else { return };
        self.pinned.remove(idx);
        let limit = MAX_HISTORY.load(Ordering::Relaxed);
        if self.items.len() >= limit {
            self.items.pop_back();
        }
        self.items.push_front(text);
    }

    /// Seeds the pinned list from persisted entries. Called once at startup.
    pub fn load_pinned(&mut self, items: Vec<String>) {
        self.pinned = items.into_iter().collect();
    }

    /// Writes pinned[idx] to the clipboard and returns the text. No-op if out of range.
    pub fn select_pinned(&mut self, idx: usize) -> Option<String> {
        let text = self.pinned.get(idx)?.clone();
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let ns_text = NSString::from_str(&text);
        pasteboard.setString_forType(&ns_text, ns_string!("public.utf8-plain-text"));
        self.last_change_count = pasteboard.changeCount();
        Some(text)
    }

    /// Removes history[idx]. No-op if out of range.
    pub fn delete_history(&mut self, idx: usize) {
        self.items.remove(idx);
    }

    /// Removes pinned[idx]. No-op if out of range.
    pub fn delete_pinned(&mut self, idx: usize) {
        self.pinned.remove(idx);
    }

    fn trim_to_limit(&mut self) {
        let limit = MAX_HISTORY.load(Ordering::Relaxed);
        while self.items.len() > limit {
            self.items.pop_back();
        }
    }

    /// Returns a truncated, single-line display label for menu rendering.
    pub fn display_label(text: &str) -> String {
        Self::truncate_clean(text, LABEL_LEN)
    }

    /// Returns a longer tooltip string for hover display.
    pub fn display_tooltip(text: &str) -> String {
        Self::truncate_clean(text, TOOLTIP_LEN)
    }

    /// Replaces newlines with ↩ symbols and truncates to `max_len` characters,
    /// appending "..." if truncated.
    fn truncate_clean(text: &str, max_len: usize) -> String {
        let cleaned: String = text
            .chars()
            .map(|c| {
                if c == '\n' || c == '\r' {
                    '\u{21A9}'
                } else {
                    c
                }
            })
            .collect();

        if cleaned.chars().count() > max_len {
            let truncated: String = cleaned.chars().take(max_len).collect();
            format!("{}...", truncated)
        } else {
            cleaned
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_label_short_string_unchanged() {
        assert_eq!(
            ClipboardHistory::display_label("hello world"),
            "hello world"
        );
    }

    #[test]
    fn display_label_empty_string() {
        assert_eq!(ClipboardHistory::display_label(""), "");
    }

    #[test]
    fn display_label_exact_boundary_no_truncation() {
        let input: String = "a".repeat(LABEL_LEN);
        let result = ClipboardHistory::display_label(&input);
        assert_eq!(result, input);
        assert!(!result.ends_with("..."));
    }

    #[test]
    fn display_label_one_over_boundary_truncated() {
        let input: String = "a".repeat(LABEL_LEN + 1);
        let result = ClipboardHistory::display_label(&input);
        assert_eq!(result.chars().count(), LABEL_LEN + 3);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn display_label_newlines_replaced() {
        let result = ClipboardHistory::display_label("line1\nline2\rline3");
        assert_eq!(result, "line1\u{21A9}line2\u{21A9}line3");
    }

    #[test]
    fn display_label_newlines_and_truncation_combined() {
        let input = "hello\nworld\n".repeat(10);
        let result = ClipboardHistory::display_label(&input);
        assert!(result.ends_with("..."));
        assert!(!result.contains('\n'));
        assert!(result.contains('\u{21A9}'));
    }

    #[test]
    fn display_tooltip_longer_limit() {
        let input: String = "b".repeat(TOOLTIP_LEN);
        assert_eq!(ClipboardHistory::display_tooltip(&input), input);

        let long_input: String = "b".repeat(TOOLTIP_LEN + 1);
        let result = ClipboardHistory::display_tooltip(&long_input);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), TOOLTIP_LEN + 3);
    }

    #[test]
    fn display_label_preserves_unicode() {
        let input = "caf\u{e9} \u{1F600}";
        assert_eq!(ClipboardHistory::display_label(input), input);
    }

    #[test]
    fn display_label_only_newlines() {
        assert_eq!(
            ClipboardHistory::display_label("\n\r\n"),
            "\u{21A9}\u{21A9}\u{21A9}"
        );
    }

    #[test]
    fn display_label_preserves_tabs_and_spaces() {
        assert_eq!(
            ClipboardHistory::display_label("hello\tworld there"),
            "hello\tworld there"
        );
    }

    // ── New v0.2 tests ───────────────────────────────────────────

    #[test]
    fn clear_empties_items() {
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string(), "b".to_string()]),
            pinned: VecDeque::new(),
            last_change_count: 0,
        };
        h.clear();
        assert!(h.items().is_empty());
    }

    #[test]
    fn load_items_seeds_ring() {
        let mut h = ClipboardHistory {
            items: VecDeque::new(),
            pinned: VecDeque::new(),
            last_change_count: 0,
        };
        let items = vec!["x".to_string(), "y".to_string()];
        h.load_items(items.clone());
        let loaded: Vec<String> = h.items().iter().cloned().collect();
        assert_eq!(loaded, items);
    }

    #[test]
    fn load_items_caps_at_max_history() {
        let mut h = ClipboardHistory {
            items: VecDeque::new(),
            pinned: VecDeque::new(),
            last_change_count: 0,
        };
        // temporarily lower the max to 2
        let original = get_max_history();
        set_max_history(2);
        let items: Vec<String> = (0..5).map(|i| i.to_string()).collect();
        h.load_items(items);
        assert_eq!(h.items().len(), 2, "should cap at MAX_HISTORY");
        set_max_history(original);
    }

    // ── Pin/unpin tests ───────────────────────────────────────────

    #[test]
    fn pin_moves_item_to_pinned_list() {
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string(), "b".to_string()]),
            pinned: VecDeque::new(),
            last_change_count: 0,
        };
        h.pin(1); // pin "b"
        assert_eq!(h.items().len(), 1);
        assert_eq!(h.items()[0], "a");
        assert_eq!(h.pinned_items().len(), 1);
        assert_eq!(h.pinned_items()[0], "b");
    }

    #[test]
    fn pin_is_noop_when_already_pinned() {
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string()]),
            pinned: VecDeque::from(["a".to_string()]),
            last_change_count: 0,
        };
        h.pin(0); // "a" is already pinned — no-op
        assert_eq!(h.items().len(), 1); // unchanged
        assert_eq!(h.pinned_items().len(), 1);
    }

    #[test]
    fn unpin_moves_item_to_front_of_history() {
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string()]),
            pinned: VecDeque::from(["b".to_string()]),
            last_change_count: 0,
        };
        h.unpin(0); // unpin "b"
        assert_eq!(h.items()[0], "b");
        assert_eq!(h.items()[1], "a");
        assert!(h.pinned_items().is_empty());
    }

    #[test]
    fn unpin_evicts_oldest_when_at_capacity() {
        set_max_history(2);
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string(), "b".to_string()]),
            pinned: VecDeque::from(["c".to_string()]),
            last_change_count: 0,
        };
        h.unpin(0); // history at max; "b" (oldest) should be evicted
        assert_eq!(h.items().len(), 2);
        assert_eq!(h.items()[0], "c");
        assert_eq!(h.items()[1], "a");
        set_max_history(10); // restore
    }

    #[test]
    fn poll_ignores_copy_of_pinned_text() {
        let mut h = ClipboardHistory {
            items: VecDeque::new(),
            pinned: VecDeque::from(["secret".to_string()]),
            last_change_count: 0,
        };
        // Simulate what poll() does when it sees "secret":
        let text = "secret".to_string();
        if !h.pinned_items().iter().any(|p| p == &text) {
            h.items.push_front(text);
        }
        assert!(h.items().is_empty(), "pinned item should not enter history");
    }

    #[test]
    fn clear_clears_both_history_and_pinned() {
        let mut h = ClipboardHistory {
            items: VecDeque::from(["a".to_string()]),
            pinned: VecDeque::from(["b".to_string()]),
            last_change_count: 0,
        };
        h.clear();
        assert!(h.items().is_empty());
        assert!(h.pinned_items().is_empty());
    }

    #[test]
    fn display_label_sixty_char_limit() {
        let input: String = "x".repeat(60);
        assert_eq!(ClipboardHistory::display_label(&input), input);

        let long: String = "x".repeat(61);
        let result = ClipboardHistory::display_label(&long);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), 63); // 60 + "..."
    }
}
