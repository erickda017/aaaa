// Em release no Windows, abre sem a janela preta de console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod audio;
mod config;
mod gui;
mod hotkey;
mod shared;

use eframe::egui;

fn main() -> eframe::Result {
    let cfg = config::Config::load();
    let shared = shared::Shared::new(
        cfg.settings,
        cfg.hotkey_vk,
        cfg.hotkey_hold,
        cfg.monitor_volume,
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Voz do Diabo")
            .with_inner_size([620.0, 880.0])
            .with_min_inner_size([480.0, 440.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Voz do Diabo",
        options,
        Box::new(move |cc| {
            hotkey::spawn(shared.clone(), cc.egui_ctx.clone());
            Ok(Box::new(gui::VoiceApp::new(cc, shared, cfg)))
        }),
    )
}
