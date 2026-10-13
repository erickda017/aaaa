//! Estado compartilhado entre a interface, o atalho global e as threads de áudio.
//! Tudo aqui é lock-free (atômicos) exceto `settings`, que a thread de áudio só lê
//! com `try_lock` para nunca bloquear.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use voice_core::{Meters, Settings};

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub settings_version: AtomicU64,
    /// `true` = voz processada; `false` = voz limpa passando direto.
    pub active: AtomicBool,

    pub hotkey_vk: AtomicU32,
    pub hotkey_hold: AtomicBool,
    pub capture_request: AtomicBool,

    pub monitor_volume: AtomicU32,

    in_peak: AtomicU32,
    out_peak: AtomicU32,
    gain_reduction: AtomicU32,

    pub underruns: AtomicU64,
    pub overruns: AtomicU64,
    pub last_error: Mutex<Option<String>>,
}

fn load_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

fn store_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Relaxed)
}

impl Shared {
    pub fn new(
        settings: Settings,
        hotkey_vk: u32,
        hotkey_hold: bool,
        monitor_volume: f32,
    ) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(settings),
            settings_version: AtomicU64::new(1),
            active: AtomicBool::new(false),
            hotkey_vk: AtomicU32::new(hotkey_vk),
            hotkey_hold: AtomicBool::new(hotkey_hold),
            capture_request: AtomicBool::new(false),
            monitor_volume: AtomicU32::new(monitor_volume.to_bits()),
            in_peak: AtomicU32::new(0),
            out_peak: AtomicU32::new(0),
            gain_reduction: AtomicU32::new(0),
            underruns: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
            last_error: Mutex::new(None),
        })
    }

    pub fn set_settings(&self, s: Settings) {
        if let Ok(mut g) = self.settings.lock() {
            *g = s;
        }
        self.settings_version.fetch_add(1, Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Relaxed)
    }

    pub fn set_active(&self, on: bool) {
        self.active.store(on, Relaxed);
    }

    pub fn toggle_active(&self) {
        self.active.fetch_xor(true, Relaxed);
    }

    pub fn set_monitor_volume(&self, v: f32) {
        store_f32(&self.monitor_volume, v);
    }

    pub fn monitor_volume(&self) -> f32 {
        load_f32(&self.monitor_volume)
    }

    pub fn publish_meters(&self, m: Meters) {
        store_f32(&self.in_peak, m.input_peak);
        store_f32(&self.out_peak, m.output_peak);
        store_f32(&self.gain_reduction, m.gain_reduction_db);
    }

    pub fn meters(&self) -> Meters {
        Meters {
            input_peak: load_f32(&self.in_peak),
            output_peak: load_f32(&self.out_peak),
            gain_reduction_db: load_f32(&self.gain_reduction),
        }
    }

    pub fn set_error(&self, msg: String) {
        if let Ok(mut g) = self.last_error.lock() {
            *g = Some(msg);
        }
    }

    pub fn take_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|mut g| g.take())
    }
}
