use std::collections::VecDeque;

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSString, ns_string};

/// TODO: make this configurable by the user through a settings menu, and persist across restarts.
const MAX_HISTORY: usize = 10;
const LABEL_LEN: usize = 30;
const TOOLTIP_LEN: usize = 300;

pub struct ClipboardHistory {
    items: VecDeque<String>,
    last_change_count: isize,
}

impl ClipboardHistory {
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

        if self.items.len() > MAX_HISTORY {
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
        crate::log::log(&format!(
            "clipboard.select({}): setString_forType returned {}",
            index, ok
        ));

        // Verify the clipboard content
        let verify = pasteboard.stringForType(ns_string!("public.utf8-plain-text"));
        crate::log::log(&format!(
            "clipboard.select({}): verify read back = {:?}",
            index,
            verify.map(|s| Self::display_label(&s.to_string()))
        ));

        self.last_change_count = pasteboard.changeCount();
        crate::log::log(&format!(
            "clipboard.select({}): changeCount now = {}",
            index, self.last_change_count
        ));

        Some(text)
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
