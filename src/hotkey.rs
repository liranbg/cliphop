use global_hotkey::{
    GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};

pub struct Hotkey {
    manager: GlobalHotKeyManager,
    pub hotkey: HotKey,
}

impl Hotkey {
    pub fn new() -> Self {
        let manager = GlobalHotKeyManager::new().expect("Failed to create hotkey manager");
        // Option (Alt) + V
        let hotkey = HotKey::new(Some(Modifiers::ALT), Code::KeyV);
        manager.register(hotkey).expect("Failed to register hotkey");
        Self { manager, hotkey }
    }

    /// Unregisters the current hotkey and registers a new one from a config string.
    /// Returns Err if the combo string is invalid or registration fails.
    pub fn re_register(&mut self, combo: &str) -> Result<(), String> {
        self.manager.unregister(self.hotkey).map_err(|e| e.to_string())?;
        let (mods, code) = parse_combo(combo)?;
        let new_hotkey = HotKey::new(mods, code);
        self.manager.register(new_hotkey).map_err(|e| e.to_string())?;
        self.hotkey = new_hotkey;
        Ok(())
    }
}

/// Parses a config hotkey string (e.g. "alt+v", "ctrl+shift+f1") into
/// (Option<Modifiers>, Code). Modifier order is ignored during parsing.
pub fn parse_combo(s: &str) -> Result<(Option<Modifiers>, Code), String> {
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() {
        return Err("empty combo".to_string());
    }
    let key_str = *parts.last().unwrap();
    let code = str_to_code(key_str)?;

    let mut mods = Modifiers::empty();
    for &part in &parts[..parts.len() - 1] {
        match part {
            "meta"  => mods |= Modifiers::META,
            "ctrl"  => mods |= Modifiers::CONTROL,
            "alt"   => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            other   => return Err(format!("unknown modifier: {}", other)),
        }
    }
    Ok((if mods.is_empty() { None } else { Some(mods) }, code))
}

fn str_to_code(s: &str) -> Result<Code, String> {
    match s {
        "a" => Ok(Code::KeyA), "b" => Ok(Code::KeyB), "c" => Ok(Code::KeyC),
        "d" => Ok(Code::KeyD), "e" => Ok(Code::KeyE), "f" => Ok(Code::KeyF),
        "g" => Ok(Code::KeyG), "h" => Ok(Code::KeyH), "i" => Ok(Code::KeyI),
        "j" => Ok(Code::KeyJ), "k" => Ok(Code::KeyK), "l" => Ok(Code::KeyL),
        "m" => Ok(Code::KeyM), "n" => Ok(Code::KeyN), "o" => Ok(Code::KeyO),
        "p" => Ok(Code::KeyP), "q" => Ok(Code::KeyQ), "r" => Ok(Code::KeyR),
        "s" => Ok(Code::KeyS), "t" => Ok(Code::KeyT), "u" => Ok(Code::KeyU),
        "v" => Ok(Code::KeyV), "w" => Ok(Code::KeyW), "x" => Ok(Code::KeyX),
        "y" => Ok(Code::KeyY), "z" => Ok(Code::KeyZ),
        "0" => Ok(Code::Digit0), "1" => Ok(Code::Digit1), "2" => Ok(Code::Digit2),
        "3" => Ok(Code::Digit3), "4" => Ok(Code::Digit4), "5" => Ok(Code::Digit5),
        "6" => Ok(Code::Digit6), "7" => Ok(Code::Digit7), "8" => Ok(Code::Digit8),
        "9" => Ok(Code::Digit9),
        "f1"  => Ok(Code::F1),  "f2"  => Ok(Code::F2),  "f3"  => Ok(Code::F3),
        "f4"  => Ok(Code::F4),  "f5"  => Ok(Code::F5),  "f6"  => Ok(Code::F6),
        "f7"  => Ok(Code::F7),  "f8"  => Ok(Code::F8),  "f9"  => Ok(Code::F9),
        "f10" => Ok(Code::F10), "f11" => Ok(Code::F11), "f12" => Ok(Code::F12),
        "space" => Ok(Code::Space),
        "tab"   => Ok(Code::Tab),
        "comma" => Ok(Code::Comma),
        other => Err(format!("unknown key: {}", other)),
    }
}

fn code_to_str(code: Code) -> &'static str {
    match code {
        Code::KeyA => "a", Code::KeyB => "b", Code::KeyC => "c",
        Code::KeyD => "d", Code::KeyE => "e", Code::KeyF => "f",
        Code::KeyG => "g", Code::KeyH => "h", Code::KeyI => "i",
        Code::KeyJ => "j", Code::KeyK => "k", Code::KeyL => "l",
        Code::KeyM => "m", Code::KeyN => "n", Code::KeyO => "o",
        Code::KeyP => "p", Code::KeyQ => "q", Code::KeyR => "r",
        Code::KeyS => "s", Code::KeyT => "t", Code::KeyU => "u",
        Code::KeyV => "v", Code::KeyW => "w", Code::KeyX => "x",
        Code::KeyY => "y", Code::KeyZ => "z",
        Code::Digit0 => "0", Code::Digit1 => "1", Code::Digit2 => "2",
        Code::Digit3 => "3", Code::Digit4 => "4", Code::Digit5 => "5",
        Code::Digit6 => "6", Code::Digit7 => "7", Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::F1  => "f1",  Code::F2  => "f2",  Code::F3  => "f3",
        Code::F4  => "f4",  Code::F5  => "f5",  Code::F6  => "f6",
        Code::F7  => "f7",  Code::F8  => "f8",  Code::F9  => "f9",
        Code::F10 => "f10", Code::F11 => "f11", Code::F12 => "f12",
        Code::Space => "space",
        Code::Tab   => "tab",
        Code::Comma => "comma",
        _ => "unknown",
    }
}

/// Serialises a combo to canonical config format: `meta+ctrl+alt+shift+<key>`.
pub fn format_combo(modifiers: Option<Modifiers>, code: Code) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(m) = modifiers {
        if m.contains(Modifiers::META)    { parts.push("meta");  }
        if m.contains(Modifiers::CONTROL) { parts.push("ctrl");  }
        if m.contains(Modifiers::ALT)     { parts.push("alt");   }
        if m.contains(Modifiers::SHIFT)   { parts.push("shift"); }
    }
    parts.push(code_to_str(code));
    parts.join("+")
}

/// Converts a raw NSEventModifierFlags bitmask and a key character (from
/// `NSEvent.charactersIgnoringModifiers`) into a canonical combo string.
/// Returns Err if the key cannot be mapped.
pub fn combo_from_event_flags(modifier_bits: u64, chars_ignoring_mods: &str) -> Result<String, String> {
    // NSEventModifierFlags bit values (from AppKit headers)
    const CMD:   u64 = 1 << 20; // NSEventModifierFlagCommand
    const SHIFT: u64 = 1 << 17; // NSEventModifierFlagShift
    const OPT:   u64 = 1 << 19; // NSEventModifierFlagOption (Alt)
    const CTRL:  u64 = 1 << 18; // NSEventModifierFlagControl

    let key_char = chars_ignoring_mods.to_lowercase();
    let code = str_to_code(key_char.trim())?;

    let mut mods = Modifiers::empty();
    if modifier_bits & CMD   != 0 { mods |= Modifiers::META; }
    if modifier_bits & CTRL  != 0 { mods |= Modifiers::CONTROL; }
    if modifier_bits & OPT   != 0 { mods |= Modifiers::ALT; }
    if modifier_bits & SHIFT != 0 { mods |= Modifiers::SHIFT; }

    if mods.is_empty() {
        return Err("combo must include at least one modifier key".to_string());
    }

    Ok(format_combo(Some(mods), code))
}

/// Returns a display string for a hotkey combo config (e.g. "alt+v" → "⌥V").
pub fn display_combo(combo: &str) -> String {
    let Ok((mods, code)) = parse_combo(combo) else {
        return combo.to_string();
    };
    let mut s = String::new();
    if let Some(m) = mods {
        if m.contains(Modifiers::META)    { s.push('⌘'); }
        if m.contains(Modifiers::CONTROL) { s.push('⌃'); }
        if m.contains(Modifiers::ALT)     { s.push('⌥'); }
        if m.contains(Modifiers::SHIFT)   { s.push('⇧'); }
    }
    s.push_str(&code_to_str(code).to_uppercase());
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_combo() {
        let (mods, code) = parse_combo("alt+v").unwrap();
        assert_eq!(mods, Some(Modifiers::ALT));
        assert_eq!(code, Code::KeyV);
    }

    #[test]
    fn parse_multi_modifier() {
        let (mods, code) = parse_combo("ctrl+shift+f1").unwrap();
        assert_eq!(mods, Some(Modifiers::CONTROL | Modifiers::SHIFT));
        assert_eq!(code, Code::F1);
    }

    #[test]
    fn parse_meta_combo() {
        let (mods, code) = parse_combo("meta+k").unwrap();
        assert_eq!(mods, Some(Modifiers::META));
        assert_eq!(code, Code::KeyK);
    }

    #[test]
    fn parse_unknown_modifier_returns_err() {
        assert!(parse_combo("super+v").is_err());
    }

    #[test]
    fn parse_unknown_key_returns_err() {
        assert!(parse_combo("alt+pageup").is_err());
    }

    #[test]
    fn format_round_trips() {
        let cases = ["alt+v", "meta+shift+p", "ctrl+alt+f1", "alt+space"];
        for &combo in &cases {
            let (mods, code) = parse_combo(combo).unwrap();
            assert_eq!(format_combo(mods, code), combo, "round-trip failed for {combo}");
        }
    }
}
