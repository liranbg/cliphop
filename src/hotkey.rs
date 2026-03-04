use global_hotkey::{
    GlobalHotKeyManager,
    hotkey::{Code, HotKey, Modifiers},
};

pub struct Hotkey {
    // Must stay alive to keep the hotkey registered
    _manager: GlobalHotKeyManager,
    pub hotkey: HotKey,
}

impl Hotkey {
    pub fn new() -> Self {
        let manager = GlobalHotKeyManager::new().expect("Failed to create hotkey manager");
        // Option (Alt) + V
        let hotkey = HotKey::new(Some(Modifiers::ALT), Code::KeyV);
        manager.register(hotkey).expect("Failed to register hotkey");
        Self {
            _manager: manager,
            hotkey,
        }
    }
}
