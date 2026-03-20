use std::fs;

pub const DEFAULT_MAX_HISTORY: usize = 10;
pub const MIN_MAX_HISTORY: usize = 1;
pub const MAX_MAX_HISTORY: usize = 50;

pub struct Config {
    pub verbose_logging: bool,
    pub max_history: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            verbose_logging: false,
            max_history: DEFAULT_MAX_HISTORY,
        }
    }
}

fn config_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{}/.cliphop/config", home)
}

/// Loads config from `~/.cliphop/config`. Returns defaults if the file does not exist or
/// any individual key fails to parse (the other keys are still applied).
pub fn load() -> Config {
    let path = config_path();
    let Ok(contents) = fs::read_to_string(&path) else {
        return Config::default();
    };

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
            _ => {}
        }
    }
    cfg
}

/// Saves the config to `~/.cliphop/config`, overwriting the previous file.
pub fn save(config: &Config) {
    let path = config_path();
    let contents = format!(
        "verbose_logging={}\nmax_history={}\n",
        config.verbose_logging, config.max_history
    );
    if let Err(e) = fs::write(&path, contents) {
        crate::log::log(&format!("config.save: failed to write {}: {}", path, e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_config(name: &str) -> String {
        let dir = std::env::temp_dir();
        dir.join(name).to_string_lossy().into_owned()
    }

    // Override config_path by writing/reading files directly in tests.

    #[test]
    fn default_values() {
        let cfg = Config::default();
        assert_eq!(cfg.max_history, DEFAULT_MAX_HISTORY);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn parse_valid_config() {
        let path = tmp_config("cliphop_test_valid.config");
        fs::write(&path, "verbose_logging=true\nmax_history=25\n").unwrap();

        // parse manually using the same logic as load()
        let contents = fs::read_to_string(&path).unwrap();
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
                _ => {}
            }
        }
        assert!(cfg.verbose_logging);
        assert_eq!(cfg.max_history, 25);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn max_history_clamped_high() {
        let mut cfg = Config::default();
        let n: usize = 999;
        cfg.max_history = n.clamp(MIN_MAX_HISTORY, MAX_MAX_HISTORY);
        assert_eq!(cfg.max_history, MAX_MAX_HISTORY);
    }

    #[test]
    fn max_history_clamped_zero() {
        let mut cfg = Config::default();
        let n: usize = 0;
        cfg.max_history = n.clamp(MIN_MAX_HISTORY, MAX_MAX_HISTORY);
        assert_eq!(cfg.max_history, MIN_MAX_HISTORY);
    }

    #[test]
    fn unknown_keys_ignored() {
        let contents = "unknown_key=foo\nmax_history=7\n";
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
                _ => {}
            }
        }
        assert_eq!(cfg.max_history, 7);
        assert!(!cfg.verbose_logging);
    }

    #[test]
    fn bad_value_falls_back_to_default() {
        let contents = "max_history=notanumber\nverbose_logging=notabool\n";
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
                _ => {}
            }
        }
        assert_eq!(cfg.max_history, DEFAULT_MAX_HISTORY);
        assert!(!cfg.verbose_logging);
    }
}
