//! Configuração persistente (JSON em %APPDATA%\voz-do-diabo\config.json).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use voice_core::Settings;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserPreset {
    pub name: String,
    pub settings: Settings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub input_id: Option<String>,
    pub output_id: Option<String>,
    pub monitor_id: Option<String>,
    pub monitor_enabled: bool,
    pub monitor_volume: f32,
    /// Colchão do buffer de saída (estabilidade x latência).
    pub buffer_ms: f32,
    pub hotkey_vk: u32,
    pub hotkey_hold: bool,
    pub always_on_top: bool,
    pub auto_start: bool,
    pub settings: Settings,
    pub user_presets: Vec<UserPreset>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_id: None,
            output_id: None,
            monitor_id: None,
            monitor_enabled: false,
            monitor_volume: 0.8,
            buffer_ms: 35.0,
            hotkey_vk: 0x77, // F8
            hotkey_hold: false,
            always_on_top: false,
            auto_start: true,
            settings: Settings::diabo(),
            user_presets: Vec::new(),
        }
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("voz-do-diabo").join("config.json"))
}

impl Config {
    pub fn load() -> Self {
        let Some(p) = path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&p) else {
            return Self::default();
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(mut c) => {
                c.settings = c.settings.sanitized();
                for u in &mut c.user_presets {
                    u.settings = u.settings.sanitized();
                }
                c.buffer_ms = c.buffer_ms.clamp(15.0, 200.0);
                c.monitor_volume = c.monitor_volume.clamp(0.0, 1.5);
                c
            }
            Err(e) => {
                // Mantém o arquivo ruim como backup em vez de sobrescrevê-lo em silêncio.
                eprintln!("config inválida ({e}); usando padrões");
                let _ = std::fs::rename(&p, p.with_extension("json.bak"));
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let Ok(text) = serde_json::to_string_pretty(self) else {
            return;
        };
        // grava em arquivo temporário e renomeia, para não corromper em caso de crash
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}
