use base64::{Engine, engine::general_purpose::STANDARD as B64};

pub enum HistoryEntry {
    Text(String),
    PinnedText(String),
}

fn history_path() -> String {
    format!("{}/history", crate::config::cliphop_dir())
}

/// Production API: loads from ~/.cliphop/history using Keychain key.
pub fn load() -> Vec<HistoryEntry> {
    match crate::crypto::get_or_create_key() {
        Ok(key) => load_from(&history_path(), key),
        Err(e) => {
            crate::log::log(&format!("history.load: Keychain error: {:?}", e));
            Vec::new()
        }
    }
}

/// Production API: saves all items to ~/.cliphop/history using Keychain key.
pub fn save_all(history: &[String], pinned: &[String]) {
    match crate::crypto::get_or_create_key() {
        Ok(key) => save_all_to(&history_path(), key, history, pinned),
        Err(e) => crate::log::log(&format!("history.save_all: Keychain error: {:?}", e)),
    }
}

/// Production API: deletes ~/.cliphop/history.
pub fn clear() {
    clear_file(&history_path());
}

// ── Injectable implementations (used by both production wrappers and tests) ──

pub fn load_from(path: &str, key: [u8; 32]) -> Vec<HistoryEntry> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            crate::log::log(&format!("history.load_from: read error: {}", e));
            return Vec::new();
        }
    };

    let arr: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(e) => {
            crate::log::log(&format!("history.load_from: JSON parse error: {}", e));
            return Vec::new();
        }
    };

    let entries_json = match arr.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };

    // Use a generous upper bound to prevent pathological files; the caller
    // (ClipboardHistory::load_items) enforces the user-configured cap.
    const MAX_ENTRIES: usize = 1000;
    let mut results = Vec::new();
    let mut failed = 0usize;

    for entry in entries_json.iter().take(MAX_ENTRIES) {
        let kind = entry["kind"].as_str().unwrap_or("");

        let is_pinned = match kind {
            "text" => false,
            "pinned_text" => true,
            _ => {
                crate::log::log_verbose(&format!(
                    "history.load_from: skipping unknown kind '{}'",
                    kind
                ));
                continue;
            }
        };

        let nonce_b64 = match entry["n"].as_str() {
            Some(s) => s,
            None => {
                failed += 1;
                continue;
            }
        };
        let data_b64 = match entry["d"].as_str() {
            Some(s) => s,
            None => {
                failed += 1;
                continue;
            }
        };

        let nonce_bytes = match B64.decode(nonce_b64) {
            Ok(b) if b.len() == 12 => {
                let mut a = [0u8; 12];
                a.copy_from_slice(&b);
                a
            }
            _ => {
                failed += 1;
                continue;
            }
        };

        let ciphertext = match B64.decode(data_b64) {
            Ok(b) => b,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        match crate::crypto::decrypt(&key, &nonce_bytes, &ciphertext) {
            Ok(plaintext) => {
                if let Ok(text) = String::from_utf8(plaintext) {
                    if is_pinned {
                        results.push(HistoryEntry::PinnedText(text));
                    } else {
                        results.push(HistoryEntry::Text(text));
                    }
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    if failed > 0 && results.is_empty() && !entries_json.is_empty() {
        crate::log::log(
            "WARNING: history.load_from: Keychain key mismatch — \
             history unreadable, starting fresh",
        );
    } else if failed > 0 {
        crate::log::log(&format!(
            "history.load_from: skipped {} corrupt/undecryptable entries",
            failed
        ));
    }

    results
}

pub fn save_all_to(path: &str, key: [u8; 32], history: &[String], pinned: &[String]) {
    let dir = cliphop_dir_for(path);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        crate::log::log(&format!("history.save_all_to: mkdir failed: {}", e));
        return;
    }

    let mut arr = Vec::new();

    for item in history {
        match crate::crypto::encrypt(&key, item.as_bytes()) {
            Ok((nonce, ciphertext)) => {
                arr.push(serde_json::json!({
                    "v": 1,
                    "kind": "text",
                    "n": B64.encode(nonce),
                    "d": B64.encode(&ciphertext),
                }));
            }
            Err(e) => {
                crate::log::log(&format!("history.save_all_to: encrypt error: {:?}", e));
            }
        }
    }

    for item in pinned {
        match crate::crypto::encrypt(&key, item.as_bytes()) {
            Ok((nonce, ciphertext)) => {
                arr.push(serde_json::json!({
                    "v": 1,
                    "kind": "pinned_text",
                    "n": B64.encode(nonce),
                    "d": B64.encode(&ciphertext),
                }));
            }
            Err(e) => {
                crate::log::log(&format!("history.save_all_to: encrypt error: {:?}", e));
            }
        }
    }

    let json = match serde_json::to_string(&arr) {
        Ok(s) => s,
        Err(e) => {
            crate::log::log(&format!("history.save_all_to: JSON serialize error: {}", e));
            return;
        }
    };

    let tmp = format!("{}.tmp", path);
    if let Err(e) = std::fs::write(&tmp, &json) {
        crate::log::log(&format!("history.save_all_to: write tmp failed: {}", e));
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        crate::log::log(&format!("history.save_all_to: rename failed: {}", e));
        let _ = std::fs::remove_file(&tmp);
    }
}

pub fn clear_file(path: &str) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        crate::log::log(&format!("history.clear_file: {}", e));
    }
}

fn cliphop_dir_for(history_path: &str) -> String {
    // Extract the directory component from the given path.
    std::path::Path::new(history_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(crate::config::cliphop_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_path(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn test_key() -> [u8; 32] {
        [99u8; 32]
    }

    #[test]
    fn round_trip_single_entry() {
        let path = tmp_path("cliphop_hist_test_round_trip");
        let _ = fs::remove_file(&path);
        let key = test_key();
        save_all_to(&path, key, &["hello world".to_string()], &[]);
        let entries = load_from(&path, key);
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0], HistoryEntry::Text(s) if s == "hello world"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn round_trip_preserves_order() {
        let path = tmp_path("cliphop_hist_test_order");
        let _ = fs::remove_file(&path);
        let key = test_key();
        let items = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ];
        save_all_to(&path, key, &items, &[]);
        let entries = load_from(&path, key);
        let texts: Vec<String> = entries
            .into_iter()
            .filter_map(|e| match e {
                HistoryEntry::Text(s) => Some(s),
                HistoryEntry::PinnedText(_) => None,
            })
            .collect();
        assert_eq!(texts, items);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_empty() {
        let path = tmp_path("cliphop_hist_test_missing_xyz");
        let _ = fs::remove_file(&path);
        let entries = load_from(&path, test_key());
        assert!(entries.is_empty());
    }

    #[test]
    fn wrong_key_returns_empty_with_no_panic() {
        let path = tmp_path("cliphop_hist_test_wrong_key");
        let _ = fs::remove_file(&path);
        let write_key = [1u8; 32];
        let read_key = [2u8; 32];
        save_all_to(&path, write_key, &["secret".to_string()], &[]);
        let entries = load_from(&path, read_key);
        assert!(entries.is_empty(), "wrong key must return empty, not panic");
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn clear_removes_file() {
        let path = tmp_path("cliphop_hist_test_clear");
        let key = test_key();
        save_all_to(&path, key, &["item".to_string()], &[]);
        assert!(
            fs::metadata(&path).is_ok(),
            "file should exist before clear"
        );
        clear_file(&path);
        assert!(
            fs::metadata(&path).is_err(),
            "file should be gone after clear"
        );
    }

    #[test]
    fn corrupt_entry_skipped_valid_entries_load() {
        // Write a valid file, then manually corrupt one entry
        let path = tmp_path("cliphop_hist_test_corrupt");
        let _ = fs::remove_file(&path);
        let key = test_key();
        save_all_to(&path, key, &["good1".to_string(), "good2".to_string()], &[]);
        // Inject a corrupt entry into the JSON
        let contents = fs::read_to_string(&path).unwrap();
        let mut arr: serde_json::Value = serde_json::from_str(&contents).unwrap();
        arr.as_array_mut().unwrap().push(serde_json::json!({
            "v": 1,
            "kind": "text",
            "n": "bm90YmFzZTY0",  // valid b64 but wrong length for nonce
            "d": "bm90YmFzZTY0"   // valid b64 but corrupt ciphertext
        }));
        fs::write(&path, serde_json::to_string(&arr).unwrap()).unwrap();
        let entries = load_from(&path, key);
        // 2 good entries load; corrupt one is skipped
        assert_eq!(entries.len(), 2);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn pinned_text_round_trip() {
        let path = tmp_path("cliphop_hist_test_pinned_rt");
        let _ = fs::remove_file(&path);
        let key = test_key();
        save_all_to(
            &path,
            key,
            &["regular".to_string()],
            &["pinned".to_string()],
        );
        let entries = load_from(&path, key);
        assert_eq!(entries.len(), 2);
        assert!(matches!(&entries[0], HistoryEntry::Text(s) if s == "regular"));
        assert!(matches!(&entries[1], HistoryEntry::PinnedText(s) if s == "pinned"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn pinned_text_survives_reload_with_old_kind_skipped() {
        let path = tmp_path("cliphop_hist_test_pinned_unknown");
        let _ = fs::remove_file(&path);
        let key = test_key();
        save_all_to(&path, key, &[], &["pinned_item".to_string()]);
        let contents = fs::read_to_string(&path).unwrap();
        let mut arr: serde_json::Value = serde_json::from_str(&contents).unwrap();
        arr.as_array_mut().unwrap().push(serde_json::json!({
            "v": 1, "kind": "future_kind", "n": "AAAAAAAAAA==", "d": "AAAA"
        }));
        fs::write(&path, serde_json::to_string(&arr).unwrap()).unwrap();
        let entries = load_from(&path, key);
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0], HistoryEntry::PinnedText(s) if s == "pinned_item"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn empty_pinned_list_round_trips() {
        let path = tmp_path("cliphop_hist_test_empty_pinned");
        let _ = fs::remove_file(&path);
        let key = test_key();
        save_all_to(&path, key, &["only_history".to_string()], &[]);
        let entries = load_from(&path, key);
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0], HistoryEntry::Text(_)));
        let _ = fs::remove_file(&path);
    }
}
