//! Parâmetros da voz e presets. Tudo é `Copy` para poder ser trocado de forma
//! barata entre a thread da interface e a thread de áudio.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Entrada
    pub input_gain_db: f32,
    /// Abaixo deste nível o gate fecha. -80 desliga.
    pub gate_threshold_db: f32,

    // Camadas de voz (semitons relativos à voz original; negativo = mais grave)
    pub dry_level: f32,
    pub main_semitones: f32,
    pub main_level: f32,
    pub sub_semitones: f32,
    pub sub_level: f32,
    pub ghost_semitones: f32,
    pub ghost_level: f32,
    /// Chiado/respiração que acompanha a fala (0..1).
    pub breath_level: f32,
    /// Modulação de amplitude rápida que dá rugosidade ("growl").
    pub growl_depth: f32,
    pub growl_hz: f32,
    /// Tamanho do grão do pitch shifter em ms. Maior = mais suave, mais latência.
    pub grain_ms: f32,

    // Timbre
    pub drive: f32,
    pub bass_db: f32,
    pub mud_cut_db: f32,
    pub presence_db: f32,
    pub air_db: f32,

    // Espaço
    pub reverb_mix: f32,
    pub reverb_size: f32,
    pub reverb_damp: f32,
    pub reverb_predelay_ms: f32,

    // Dinâmica / saída
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub output_gain_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings::diabo()
    }
}

impl Settings {
    /// Base neutra: tudo desligado, só a voz original passando.
    pub fn neutral() -> Self {
        Settings {
            input_gain_db: 0.0,
            gate_threshold_db: -80.0,
            dry_level: 1.0,
            main_semitones: 0.0,
            main_level: 0.0,
            sub_semitones: -12.0,
            sub_level: 0.0,
            ghost_semitones: 0.0,
            ghost_level: 0.0,
            breath_level: 0.0,
            growl_depth: 0.0,
            growl_hz: 45.0,
            grain_ms: 32.0,
            drive: 0.0,
            bass_db: 0.0,
            mud_cut_db: 0.0,
            presence_db: 0.0,
            air_db: 0.0,
            reverb_mix: 0.0,
            reverb_size: 0.5,
            reverb_damp: 0.5,
            reverb_predelay_ms: 15.0,
            comp_threshold_db: -18.0,
            comp_ratio: 2.5,
            output_gain_db: 0.0,
        }
    }

    /// O Diabo: voz principal bem grave, uma camada uma oitava abaixo, um "fantasma"
    /// levemente desafinado criando dobra, sopro, aspereza e um espaço escuro.
    pub fn diabo() -> Self {
        Settings {
            gate_threshold_db: -52.0,
            dry_level: 0.10,
            main_semitones: -6.0,
            main_level: 1.0,
            sub_semitones: -12.0,
            sub_level: 0.55,
            ghost_semitones: -5.4,
            ghost_level: 0.45,
            breath_level: 0.10,
            growl_depth: 0.18,
            growl_hz: 42.0,
            drive: 0.35,
            bass_db: 5.0,
            mud_cut_db: -3.0,
            presence_db: 3.0,
            air_db: -2.0,
            reverb_mix: 0.20,
            reverb_size: 0.65,
            reverb_damp: 0.55,
            reverb_predelay_ms: 28.0,
            comp_threshold_db: -20.0,
            comp_ratio: 3.5,
            output_gain_db: 2.0,
            ..Self::neutral()
        }
    }

    /// Sussurro sobrenatural: pouco grave, muito sopro e eco.
    pub fn sussurro() -> Self {
        Settings {
            gate_threshold_db: -55.0,
            dry_level: 0.35,
            main_semitones: -3.0,
            main_level: 0.7,
            sub_level: 0.0,
            ghost_semitones: -2.4,
            ghost_level: 0.4,
            breath_level: 0.55,
            growl_depth: 0.0,
            drive: 0.10,
            bass_db: 1.0,
            mud_cut_db: -4.0,
            presence_db: 2.0,
            air_db: 3.0,
            reverb_mix: 0.40,
            reverb_size: 0.85,
            reverb_damp: 0.35,
            reverb_predelay_ms: 45.0,
            comp_threshold_db: -24.0,
            comp_ratio: 4.0,
            output_gain_db: 4.0,
            ..Self::neutral()
        }
    }

    /// Entidade abissal: o mais grave possível, rugosa e pesada.
    pub fn abissal() -> Self {
        Settings {
            gate_threshold_db: -50.0,
            dry_level: 0.0,
            main_semitones: -9.0,
            main_level: 1.0,
            sub_semitones: -16.0,
            sub_level: 0.70,
            ghost_semitones: -8.3,
            ghost_level: 0.55,
            breath_level: 0.14,
            growl_depth: 0.30,
            growl_hz: 36.0,
            drive: 0.55,
            bass_db: 6.0,
            mud_cut_db: -4.0,
            presence_db: 2.0,
            air_db: -4.0,
            reverb_mix: 0.28,
            reverb_size: 0.80,
            reverb_damp: 0.65,
            reverb_predelay_ms: 35.0,
            comp_threshold_db: -22.0,
            comp_ratio: 4.0,
            output_gain_db: 3.0,
            ..Self::neutral()
        }
    }

    /// Voz "engrossada" discreta, para falas que precisam de um pouco de peso.
    pub fn sombrio() -> Self {
        Settings {
            gate_threshold_db: -55.0,
            dry_level: 0.30,
            main_semitones: -3.5,
            main_level: 0.85,
            sub_semitones: -12.0,
            sub_level: 0.20,
            drive: 0.12,
            bass_db: 3.0,
            mud_cut_db: -2.0,
            presence_db: 1.5,
            reverb_mix: 0.08,
            reverb_size: 0.4,
            comp_threshold_db: -20.0,
            comp_ratio: 3.0,
            output_gain_db: 1.0,
            ..Self::neutral()
        }
    }

    /// Força todos os valores para faixas seguras (arquivos de config corrompidos,
    /// NaN, etc.).
    pub fn sanitized(mut self) -> Self {
        fn c(v: &mut f32, lo: f32, hi: f32, default: f32) {
            *v = if v.is_finite() { v.clamp(lo, hi) } else { default };
        }
        let d = Settings::neutral();
        c(&mut self.input_gain_db, -24.0, 24.0, d.input_gain_db);
        c(&mut self.gate_threshold_db, -80.0, -10.0, d.gate_threshold_db);
        c(&mut self.dry_level, 0.0, 2.0, d.dry_level);
        c(&mut self.main_semitones, -24.0, 12.0, d.main_semitones);
        c(&mut self.main_level, 0.0, 2.0, d.main_level);
        c(&mut self.sub_semitones, -36.0, 12.0, d.sub_semitones);
        c(&mut self.sub_level, 0.0, 2.0, d.sub_level);
        c(&mut self.ghost_semitones, -24.0, 12.0, d.ghost_semitones);
        c(&mut self.ghost_level, 0.0, 2.0, d.ghost_level);
        c(&mut self.breath_level, 0.0, 1.0, d.breath_level);
        c(&mut self.growl_depth, 0.0, 1.0, d.growl_depth);
        c(&mut self.growl_hz, 10.0, 120.0, d.growl_hz);
        c(&mut self.grain_ms, 15.0, 80.0, d.grain_ms);
        c(&mut self.drive, 0.0, 1.0, d.drive);
        c(&mut self.bass_db, -12.0, 12.0, d.bass_db);
        c(&mut self.mud_cut_db, -12.0, 6.0, d.mud_cut_db);
        c(&mut self.presence_db, -12.0, 12.0, d.presence_db);
        c(&mut self.air_db, -12.0, 12.0, d.air_db);
        c(&mut self.reverb_mix, 0.0, 1.0, d.reverb_mix);
        c(&mut self.reverb_size, 0.0, 1.0, d.reverb_size);
        c(&mut self.reverb_damp, 0.0, 0.95, d.reverb_damp);
        c(&mut self.reverb_predelay_ms, 0.0, 200.0, d.reverb_predelay_ms);
        c(&mut self.comp_threshold_db, -50.0, 0.0, d.comp_threshold_db);
        c(&mut self.comp_ratio, 1.0, 20.0, d.comp_ratio);
        c(&mut self.output_gain_db, -24.0, 24.0, d.output_gain_db);
        self
    }
}

/// Presets embutidos, na ordem em que aparecem na interface.
pub fn builtin_presets() -> Vec<(&'static str, Settings)> {
    vec![
        ("Diabo", Settings::diabo()),
        ("Abissal", Settings::abissal()),
        ("Sussurro", Settings::sussurro()),
        ("Sombrio (sutil)", Settings::sombrio()),
    ]
}
