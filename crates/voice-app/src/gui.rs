//! Interface (egui).

use crate::audio::{self, DeviceInfo, DeviceLists, Running, StartConfig};
use crate::config::{Config, UserPreset};
use crate::hotkey;
use crate::shared::Shared;
use eframe::egui::{self, Color32, RichText};
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};
use voice_core::{Settings, builtin_presets};

const RED: Color32 = Color32::from_rgb(170, 28, 28);
const GREY: Color32 = Color32::from_rgb(70, 74, 82);

pub struct VoiceApp {
    cfg: Config,
    shared: Arc<Shared>,
    devices: DeviceLists,
    running: Option<Running>,
    error: Option<String>,
    new_preset_name: String,
    dirty: bool,
    last_save: Instant,
    restart: bool,
    on_top_applied: bool,
}

impl VoiceApp {
    pub fn new(cc: &eframe::CreationContext<'_>, shared: Arc<Shared>, mut cfg: Config) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.selection.bg_fill = RED;
        visuals.hyperlink_color = Color32::from_rgb(230, 90, 90);
        cc.egui_ctx.set_visuals(visuals);

        let devices = audio::list_devices();
        let known = |list: &[DeviceInfo], id: &Option<String>| {
            id.as_ref().is_some_and(|i| list.iter().any(|d| &d.id == i))
        };

        if !known(&devices.inputs, &cfg.input_id) {
            cfg.input_id = devices.default_input.clone();
        }
        if !known(&devices.outputs, &cfg.output_id) {
            // Só pré-seleciona o cabo virtual: nunca as caixas de som (microfonia!).
            cfg.output_id = audio::find_virtual_cable(&devices.outputs).map(|d| d.id.clone());
        }
        if !known(&devices.outputs, &cfg.monitor_id) {
            cfg.monitor_id = devices.default_output.clone();
        }

        shared.set_settings(cfg.settings);
        let mut app = Self {
            cfg,
            shared,
            devices,
            running: None,
            error: None,
            new_preset_name: String::new(),
            dirty: false,
            last_save: Instant::now(),
            restart: false,
            on_top_applied: false,
        };
        if app.cfg.auto_start && app.cfg.input_id.is_some() && app.cfg.output_id.is_some() {
            app.start();
        }
        app
    }

    fn start(&mut self) {
        self.running = None; // para o áudio anterior antes de abrir de novo
        let (Some(input_id), Some(output_id)) =
            (self.cfg.input_id.clone(), self.cfg.output_id.clone())
        else {
            self.error = Some("Escolha o microfone e a saída (cabo virtual).".into());
            return;
        };
        let monitor_id = if self.cfg.monitor_enabled {
            self.cfg.monitor_id.clone()
        } else {
            None
        };
        match audio::start(
            self.shared.clone(),
            &StartConfig {
                input_id,
                output_id,
                monitor_id,
                buffer_ms: self.cfg.buffer_ms,
            },
        ) {
            Ok(r) => {
                self.running = Some(r);
                self.error = None;
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn stop(&mut self) {
        self.running = None;
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn device_combo(
        ui: &mut egui::Ui,
        id: &str,
        list: &[DeviceInfo],
        selected: &mut Option<String>,
    ) -> bool {
        let current = selected
            .as_ref()
            .and_then(|s| list.iter().find(|d| &d.id == s))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| "— escolha —".into());
        let mut changed = false;
        egui::ComboBox::from_id_salt(id)
            .width(ui.available_width().min(380.0))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for d in list {
                    let is_sel = selected.as_deref() == Some(d.id.as_str());
                    if ui.selectable_label(is_sel, &d.name).clicked() && !is_sel {
                        *selected = Some(d.id.clone());
                        changed = true;
                    }
                }
            });
        changed
    }

    fn slider(
        ui: &mut egui::Ui,
        changed: &mut bool,
        label: &str,
        v: &mut f32,
        range: RangeInclusive<f32>,
        suffix: &str,
    ) {
        if ui
            .add(
                egui::Slider::new(v, range)
                    .text(label)
                    .suffix(suffix)
                    .fixed_decimals(2),
            )
            .changed()
        {
            *changed = true;
        }
    }

    fn level_bar(ui: &mut egui::Ui, label: &str, peak: f32) {
        let db = 20.0 * peak.max(1e-6).log10();
        let frac = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        ui.horizontal(|ui| {
            ui.add_sized([70.0, 18.0], egui::Label::new(label));
            let bar = egui::ProgressBar::new(frac)
                .desired_width(ui.available_width() - 70.0)
                .text(format!("{:.0} dB", db.max(-60.0)));
            ui.add(if db > -1.5 {
                bar.fill(Color32::from_rgb(200, 40, 40))
            } else {
                bar
            });
        });
    }

    fn status_badge(&self, ui: &mut egui::Ui) {
        let running = self.running.is_some();
        let active = self.shared.is_active();
        let (text, color) = match (running, active) {
            (false, _) => ("PARADO — clique em Iniciar", GREY),
            (true, true) => ("VOZ DO DIABO ATIVA", RED),
            (true, false) => (
                "Voz normal (efeito desligado)",
                Color32::from_rgb(40, 110, 70),
            ),
        };
        egui::Frame::new()
            .fill(color)
            .corner_radius(8)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(text)
                            .size(22.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
            });
    }

    fn section_devices(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Microfone:");
            if Self::device_combo(ui, "in", &self.devices.inputs, &mut self.cfg.input_id) {
                self.restart = true;
                self.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Saída (cabo virtual, que o Discord usa como microfone):");
        });
        if Self::device_combo(ui, "out", &self.devices.outputs, &mut self.cfg.output_id) {
            self.restart = true;
            self.dirty = true;
        }
        if audio::find_virtual_cable(&self.devices.outputs).is_none() {
            ui.colored_label(
                Color32::from_rgb(230, 170, 60),
                "Nenhum cabo virtual encontrado. Instale o VB-Cable (vb-audio.com/Cable) e clique em Atualizar.",
            );
        }
        let out_name = self
            .cfg
            .output_id
            .as_ref()
            .and_then(|s| self.devices.outputs.iter().find(|d| &d.id == s))
            .map(|d| d.name.to_lowercase());
        if let Some(n) = out_name
            && !n.contains("cable")
            && !n.contains("voicemeeter")
            && !n.contains("virtual")
        {
            ui.colored_label(
                Color32::from_rgb(230, 170, 60),
                "Atenção: essa saída não parece um cabo virtual. Se for suas caixas/fones, o Discord não ouvirá o efeito.",
            );
        }

        ui.add_space(4.0);
        if ui
            .checkbox(
                &mut self.cfg.monitor_enabled,
                "Ouvir minha voz processada (monitor)",
            )
            .changed()
        {
            self.restart = true;
            self.dirty = true;
        }
        if self.cfg.monitor_enabled {
            ui.horizontal(|ui| {
                ui.label("Fones:");
                if Self::device_combo(ui, "mon", &self.devices.outputs, &mut self.cfg.monitor_id) {
                    self.restart = true;
                    self.dirty = true;
                }
            });
            let mut vol = self.cfg.monitor_volume;
            if ui
                .add(egui::Slider::new(&mut vol, 0.0..=1.5).text("Volume do monitor"))
                .changed()
            {
                self.cfg.monitor_volume = vol;
                self.shared.set_monitor_volume(vol);
                self.dirty = true;
            }
            ui.weak("Use fones! Pelas caixas o microfone capta o som e dá microfonia.");
        }

        ui.add_space(4.0);
        let mut buf = self.cfg.buffer_ms;
        if ui
            .add(
                egui::Slider::new(&mut buf, 15.0..=150.0)
                    .suffix(" ms")
                    .text("Buffer de saída"),
            )
            .on_hover_text("Menor = menos atraso, mas pode estalar. Maior = mais estável.")
            .drag_stopped()
        {
            self.cfg.buffer_ms = buf;
            self.restart = true;
            self.dirty = true;
        } else {
            self.cfg.buffer_ms = buf;
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if self.running.is_some() {
                if ui
                    .add(egui::Button::new("Parar").min_size(egui::vec2(90.0, 28.0)))
                    .clicked()
                {
                    self.stop();
                }
            } else if ui
                .add(
                    egui::Button::new(RichText::new("Iniciar").strong())
                        .fill(RED)
                        .min_size(egui::vec2(90.0, 28.0)),
                )
                .clicked()
            {
                self.start();
            }
            if ui.button("Atualizar lista").clicked() {
                self.devices = audio::list_devices();
            }
            if ui
                .checkbox(&mut self.cfg.auto_start, "Iniciar ao abrir")
                .changed()
            {
                self.dirty = true;
            }
        });

        if let Some(r) = &self.running {
            let total = r.info.engine_latency_ms + r.info.buffer_ms;
            ui.weak(format!(
                "Atraso do app ~{:.0} ms (efeito {:.0} + buffer {:.0}) + latência do Windows/Discord. Taxas: {} para {} Hz{}",
                total,
                r.info.engine_latency_ms,
                r.info.buffer_ms,
                r.info.in_rate,
                r.info.out_rate,
                if r.info.monitor { ", monitor ligado" } else { "" }
            ));
            let m = self.shared.meters();
            Self::level_bar(ui, "Entrada", m.input_peak);
            Self::level_bar(ui, "Saída", m.output_peak);
            let (u, o) = (
                self.shared.underruns.load(Relaxed),
                self.shared.overruns.load(Relaxed),
            );
            if u + o > 0 {
                ui.weak(format!("Falhas de buffer: {u} faltas, {o} excessos — se houver estalos, aumente o buffer."));
            }
        }
    }

    fn section_activation(&mut self, ui: &mut egui::Ui) {
        let active = self.shared.is_active();
        let label = if active {
            "DESLIGAR VOZ DO DIABO"
        } else {
            "LIGAR VOZ DO DIABO"
        };
        let hold = self.cfg.hotkey_hold;
        let btn = egui::Button::new(
            RichText::new(label)
                .size(18.0)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(if active { GREY } else { RED });
        let size = egui::vec2(ui.available_width(), 44.0);
        if ui
            .add_enabled(!hold, |ui: &mut egui::Ui| ui.add_sized(size, btn))
            .clicked()
        {
            self.shared.toggle_active();
        }
        if hold {
            ui.weak("Modo segurar: o efeito fica ligado enquanto a tecla estiver pressionada.");
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Atalho global:");
            let mut vk = self.cfg.hotkey_vk;
            egui::ComboBox::from_id_salt("hk")
                .selected_text(hotkey::key_name(vk))
                .show_ui(ui, |ui| {
                    for (code, name) in hotkey::KEY_CHOICES {
                        ui.selectable_value(&mut vk, *code, *name);
                    }
                });
            if vk != self.cfg.hotkey_vk {
                self.cfg.hotkey_vk = vk;
                self.shared.hotkey_vk.store(vk, Relaxed);
                self.dirty = true;
            }
            let capturing = self.shared.capture_request.load(Relaxed);
            if ui
                .button(if capturing {
                    "Pressione uma tecla…"
                } else {
                    "Capturar tecla"
                })
                .clicked()
            {
                self.shared.capture_request.store(!capturing, Relaxed);
            }
        });
        ui.horizontal(|ui| {
            let mut hold = self.cfg.hotkey_hold;
            let a = ui
                .radio_value(&mut hold, false, "Alternar (liga/desliga)")
                .changed();
            let b = ui
                .radio_value(&mut hold, true, "Segurar (liga enquanto pressiona)")
                .changed();
            if a || b {
                self.cfg.hotkey_hold = hold;
                self.shared.hotkey_hold.store(hold, Relaxed);
                if hold {
                    self.shared.set_active(false);
                }
                self.dirty = true;
            }
        });
        if !cfg!(windows) {
            ui.weak("(O atalho global só funciona no Windows.)");
        }
    }

    fn section_presets(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        ui.horizontal_wrapped(|ui| {
            for (name, preset) in builtin_presets() {
                if ui.button(name).clicked() {
                    self.cfg.settings = preset;
                    *changed = true;
                }
            }
        });
        if !self.cfg.user_presets.is_empty() {
            ui.add_space(2.0);
            ui.weak("Seus presets:");
            let mut load = None;
            let mut delete = None;
            for (i, p) in self.cfg.user_presets.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button(&p.name).clicked() {
                        load = Some(i);
                    }
                    if ui.small_button("excluir").clicked() {
                        delete = Some(i);
                    }
                });
            }
            if let Some(i) = load {
                self.cfg.settings = self.cfg.user_presets[i].settings;
                *changed = true;
            }
            if let Some(i) = delete {
                self.cfg.user_presets.remove(i);
                self.dirty = true;
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_preset_name)
                    .hint_text("nome do preset")
                    .desired_width(160.0),
            );
            let name = self.new_preset_name.trim().to_string();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Salvar ajustes atuais"))
                .clicked()
            {
                let settings = self.cfg.settings;
                match self.cfg.user_presets.iter_mut().find(|p| p.name == name) {
                    Some(p) => p.settings = settings,
                    None => self.cfg.user_presets.push(UserPreset { name, settings }),
                }
                self.new_preset_name.clear();
                self.dirty = true;
            }
        });
    }

    fn section_sliders(&mut self, ui: &mut egui::Ui, changed: &mut bool) {
        let s: &mut Settings = &mut self.cfg.settings;

        egui::CollapsingHeader::new("Camadas de voz")
            .default_open(true)
            .show(ui, |ui| {
                Self::slider(
                    ui,
                    changed,
                    "Voz original (dry)",
                    &mut s.dry_level,
                    0.0..=1.5,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Voz principal — nível",
                    &mut s.main_level,
                    0.0..=1.5,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Voz principal — tom",
                    &mut s.main_semitones,
                    -16.0..=4.0,
                    " st",
                );
                Self::slider(
                    ui,
                    changed,
                    "Camada grave — nível",
                    &mut s.sub_level,
                    0.0..=1.5,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Camada grave — tom",
                    &mut s.sub_semitones,
                    -30.0..=0.0,
                    " st",
                );
                Self::slider(
                    ui,
                    changed,
                    "Fantasma (dobra) — nível",
                    &mut s.ghost_level,
                    0.0..=1.5,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Fantasma (dobra) — tom",
                    &mut s.ghost_semitones,
                    -16.0..=4.0,
                    " st",
                );
                Self::slider(
                    ui,
                    changed,
                    "Sopro / chiado",
                    &mut s.breath_level,
                    0.0..=1.0,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Rugosidade (growl)",
                    &mut s.growl_depth,
                    0.0..=1.0,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Velocidade do growl",
                    &mut s.growl_hz,
                    15.0..=90.0,
                    " Hz",
                );
            });
        egui::CollapsingHeader::new("Timbre")
            .default_open(true)
            .show(ui, |ui| {
                Self::slider(ui, changed, "Distorção", &mut s.drive, 0.0..=1.0, "");
                Self::slider(ui, changed, "Graves", &mut s.bass_db, -12.0..=12.0, " dB");
                Self::slider(
                    ui,
                    changed,
                    "Corte de lama (380 Hz)",
                    &mut s.mud_cut_db,
                    -12.0..=6.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Presença (2,8 kHz)",
                    &mut s.presence_db,
                    -12.0..=12.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Brilho (7 kHz+)",
                    &mut s.air_db,
                    -12.0..=12.0,
                    " dB",
                );
            });
        egui::CollapsingHeader::new("Espaço (reverb)")
            .default_open(false)
            .show(ui, |ui| {
                Self::slider(ui, changed, "Quantidade", &mut s.reverb_mix, 0.0..=1.0, "");
                Self::slider(ui, changed, "Tamanho", &mut s.reverb_size, 0.0..=1.0, "");
                Self::slider(
                    ui,
                    changed,
                    "Abafamento",
                    &mut s.reverb_damp,
                    0.0..=0.95,
                    "",
                );
                Self::slider(
                    ui,
                    changed,
                    "Pré-delay",
                    &mut s.reverb_predelay_ms,
                    0.0..=120.0,
                    " ms",
                );
            });
        egui::CollapsingHeader::new("Entrada, dinâmica e saída")
            .default_open(false)
            .show(ui, |ui| {
                Self::slider(
                    ui,
                    changed,
                    "Ganho do microfone",
                    &mut s.input_gain_db,
                    -12.0..=24.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Gate (corta ruído de fundo)",
                    &mut s.gate_threshold_db,
                    -80.0..=-20.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Compressor — limiar",
                    &mut s.comp_threshold_db,
                    -50.0..=0.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Compressor — razão",
                    &mut s.comp_ratio,
                    1.0..=12.0,
                    ":1",
                );
                Self::slider(
                    ui,
                    changed,
                    "Volume de saída",
                    &mut s.output_gain_db,
                    -18.0..=18.0,
                    " dB",
                );
                Self::slider(
                    ui,
                    changed,
                    "Tamanho do grão (qualidade x atraso)",
                    &mut s.grain_ms,
                    16.0..=70.0,
                    " ms",
                );
                ui.weak("Gate em -80 = desligado. Grão menor = menos atraso, mas mais granulado.");
            });
    }
}

impl eframe::App for VoiceApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(60));

        if let Some(e) = self.shared.take_error() {
            self.error = Some(e);
        }
        // O atalho global pode ter capturado uma tecla nova.
        let vk = self.shared.hotkey_vk.load(Relaxed);
        if vk != self.cfg.hotkey_vk {
            self.cfg.hotkey_vk = vk;
            self.dirty = true;
        }
        if self.cfg.always_on_top != self.on_top_applied {
            let level = if self.cfg.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.on_top_applied = self.cfg.always_on_top;
        }

        let mut settings_changed = false;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    self.status_badge(ui);
                    ui.add_space(8.0);
                    self.section_activation(ui);
                    ui.add_space(6.0);

                    if let Some(e) = self.error.clone() {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                Color32::from_rgb(240, 100, 100),
                                format!("Erro: {e}"),
                            );
                            if ui.small_button("ok").clicked() {
                                self.error = None;
                            }
                        });
                    }

                    egui::CollapsingHeader::new("Dispositivos")
                        .default_open(true)
                        .show(ui, |ui| self.section_devices(ui));
                    egui::CollapsingHeader::new("Presets")
                        .default_open(true)
                        .show(ui, |ui| self.section_presets(ui, &mut settings_changed));
                    ui.add_space(2.0);
                    self.section_sliders(ui, &mut settings_changed);

                    ui.add_space(8.0);
                    if ui
                        .checkbox(&mut self.cfg.always_on_top, "Janela sempre no topo")
                        .changed()
                    {
                        self.dirty = true;
                    }
                });
        });

        if settings_changed {
            self.shared.set_settings(self.cfg.settings);
            self.mark_dirty();
        }
        if self.restart {
            self.restart = false;
            if self.running.is_some() {
                self.start();
            }
        }
        if self.dirty && self.last_save.elapsed() > Duration::from_secs(1) {
            self.cfg.save();
            self.dirty = false;
            self.last_save = Instant::now();
        }
    }
}

impl Drop for VoiceApp {
    fn drop(&mut self) {
        self.cfg.save();
    }
}
