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

const LABEL_LEN: usize = 30;
const TOOLTIP_LEN: usize = 300;

pub struct ClipboardHistory {
    items: VecDeque<String>,
    last_change_count: isize,
}

impl ClipboardHistory {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let pasteboard = NSPasteboard::generalPasteboard();
        let count = pasteboard.changeCount();
        Self {
            items: VecDeque::new(),
            last_change_count: count,
        }
    }

    /// Polls the clipboard for changes. Returns true if a new item was added.
    pub fn poll(&mut self) -> bool {
        if TRIM_REQUESTED.swap(false, Ordering::Relaxed) {
            self.trim_to_limit();
        }

        let pasteboard = NSPasteboard::generalPasteboard();
        let count = pasteboard.changeCount();

        if count == self.last_change_count {
            return false;
        }
        self.last_change_count = count;

        let text = pasteboard.stringForType(ns_string!("public.utf8-plain-text"));

        let text = match text {
            Some(ns_str) => ns_str.to_string(),
            None => return false,
        };

        if text.is_empty() {
            return false;
        }

        // Deduplicate: remove existing copy and move to front
        if let Some(pos) = self.items.iter().position(|s| s == &text) {
            self.items.remove(pos);
        }

        self.items.push_front(text);

        if self.items.len() > MAX_HISTORY.load(Ordering::Relaxed) {
            self.items.pop_back();
        }

        true
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
}
