use std::fs;

pub const DEFAULT_MAX_HISTORY: usize = 10;
pub const MIN_MAX_HISTORY: usize = 1;
pub const MAX_MAX_HISTORY: usize = 50;

pub struct Config {
    pub verbose_logging: bool,
    pub max_history: usize,
    pub hotkey: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            verbose_logging: false,
            max_history: DEFAULT_MAX_HISTORY,
            hotkey: "alt+v".to_string(),
        }
    }
}

fn cliphop_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{}/.cliphop", home)
}

fn config_path() -> String {
    format!("{}/config", cliphop_dir())
}

/// Loads config from `~/.cliphop/config`. Returns defaults if the file does not exist or
/// any individual key fails to parse (the other keys are still applied).
pub fn load() -> Config {
    load_from(&config_path())
}

fn load_from(path: &str) -> Config {
    let Ok(contents) = fs::read_to_string(path) else {
        return Config::default();
    };
    parse(&contents)
}

fn parse(contents: &str) -> Config {
    let mut cfg = Config::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "verbose_logging" => {
                if let Ok(b) = value.trim().parse::<bool>() {
                    cfg.verbose_logging = b;
                }
            }
            "max_history" => {
                if let Ok(n) = value.trim().parse::<usize>() {
                    cfg.max_history = n.clamp(MIN_MAX_HISTORY, MAX_MAX_HISTORY);
                }
            }
            "hotkey" => {
                cfg.hotkey = value.trim().to_string();
            }
            _ => {}
        }
    }
    cfg
}

/// Saves the config to `~/.cliphop/config` atomically (write to `.tmp`, then rename).
pub fn save(config: &Config) {
    let dir = cliphop_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        crate::log::log(&format!("config.save: failed to create {}: {}", dir, e));
        return;
    }

    let path = config_path();
    let tmp = format!("{}.tmp", path);
    let contents = format!(
        "verbose_logging={}\nmax_history={}\nhotkey={}\n",
        config.verbose_logging, config.max_history, config.hotkey
    );
    if let Err(e) = fs::write(&tmp, &contents) {
        crate::log::log(&format!("config.save: failed to write {}: {}", tmp, e));
        return;
    }
    if let Err(e) = fs::rename(&tmp, &path) {
        crate::log::log(&format!(
            "config.save: failed to rename {} -> {}: {}",
            tmp, path, e
        ));
        let _ = fs::remove_file(&tmp);
    }
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

    #[test]
    fn default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.max_history, DEFAULT_MAX_HISTORY);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let path = tmp_path("cliphop_test_missing.config");
        let _ = fs::remove_file(&path);
        let cfg = load_from(&path);
        assert_eq!(cfg.max_history, DEFAULT_MAX_HISTORY);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn parse_valid_config() {
        let path = tmp_path("cliphop_test_valid.config");
        fs::write(&path, "verbose_logging=true\nmax_history=25\n").unwrap();
        let cfg = load_from(&path);
        assert!(cfg.verbose_logging);
        assert_eq!(cfg.max_history, 25);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn max_history_clamped_high() {
        let cfg = parse("max_history=999\n");
        assert_eq!(cfg.max_history, MAX_MAX_HISTORY);
    }

    #[test]
    fn max_history_clamped_zero() {
        let cfg = parse("max_history=0\n");
        assert_eq!(cfg.max_history, MIN_MAX_HISTORY);
    }

    #[test]
    fn unknown_keys_ignored() {
        let cfg = parse("unknown_key=foo\nmax_history=7\n");
        assert_eq!(cfg.max_history, 7);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn bad_value_falls_back_to_default() {
        let cfg = parse("max_history=notanumber\nverbose_logging=notabool\n");
        assert_eq!(cfg.max_history, DEFAULT_MAX_HISTORY);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn round_trip() {
        let path = tmp_path("cliphop_test_roundtrip.config");
        fs::write(&path, "verbose_logging=true\nmax_history=42\n").unwrap();
        let cfg = load_from(&path);
        assert!(cfg.verbose_logging);
        assert_eq!(cfg.max_history, 42);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn default_hotkey_is_alt_v() {
        let cfg = Config::default();
        assert_eq!(cfg.hotkey, "alt+v");
    }

    #[test]
    fn parse_hotkey_field() {
        let cfg = parse("hotkey=ctrl+shift+v\n");
        assert_eq!(cfg.hotkey, "ctrl+shift+v");
    }

    #[test]
    fn save_and_load_hotkey() {
        let path = tmp_path("cliphop_test_hotkey_config");
        let contents = "verbose_logging=false\nmax_history=10\nhotkey=meta+k\n";
        fs::write(&path, contents).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.hotkey, "meta+k");
        let _ = fs::remove_file(&path);
    }
}
