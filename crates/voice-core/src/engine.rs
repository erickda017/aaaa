//! Cadeia completa de processamento, amostra a amostra (mono).
//!
//! entrada → gate → [dry | principal | sub | fantasma | sopro] → growl → saturação
//!         → EQ → reverb → compressor → crossfade com bypass → limiter

use crate::biquad::Biquad;
use crate::dynamics::{Compressor, Gate, Limiter};
use crate::period::PeriodTracker;
use crate::pitch::PitchShifter;
use crate::reverb::Reverb;
use crate::settings::Settings;
use crate::util::{Noise, Smoothed, db_to_lin, time_coeff};
use std::f32::consts::TAU;

/// Medidores lidos pela interface (valores lineares 0..1, exceto `gain_reduction_db`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain_reduction_db: f32,
}

pub struct Engine {
    fs: f32,
    s: Settings,

    gate: Gate,
    tracker: PeriodTracker,
    ps_main: PitchShifter,
    ps_sub: PitchShifter,
    ps_ghost: PitchShifter,

    in_gain: Smoothed,
    dry: Smoothed,
    main: Smoothed,
    sub: Smoothed,
    ghost: Smoothed,
    breath: Smoothed,
    growl_depth: Smoothed,
    drive: Smoothed,
    reverb_mix: Smoothed,
    out_gain: Smoothed,
    /// 0 = bypass (voz limpa), 1 = voz processada.
    wet: Smoothed,

    breath_env: f32,
    breath_env_att: f32,
    breath_env_rel: f32,
    breath_hp: Biquad,
    noise: Noise,
    growl_phase: f32,

    pre_sat_hp: Biquad,
    bass: Biquad,
    mud: Biquad,
    presence: Biquad,
    air: Biquad,
    reverb: Reverb,
    comp: Compressor,
    limiter: Limiter,

    // medidores
    in_peak: f32,
    out_peak: f32,
    meter_decay: f32,
}

impl Engine {
    pub fn new(fs: f32) -> Self {
        let s = Settings::neutral();
        let mut e = Self {
            fs,
            s,
            gate: Gate::new(fs),
            tracker: PeriodTracker::new(fs),
            ps_main: PitchShifter::new(fs, s.grain_ms),
            ps_sub: PitchShifter::new(fs, s.grain_ms),
            ps_ghost: PitchShifter::new(fs, s.grain_ms),
            in_gain: Smoothed::new(1.0, 20.0, fs),
            dry: Smoothed::new(1.0, 20.0, fs),
            main: Smoothed::new(0.0, 20.0, fs),
            sub: Smoothed::new(0.0, 20.0, fs),
            ghost: Smoothed::new(0.0, 20.0, fs),
            breath: Smoothed::new(0.0, 30.0, fs),
            growl_depth: Smoothed::new(0.0, 30.0, fs),
            drive: Smoothed::new(0.0, 30.0, fs),
            reverb_mix: Smoothed::new(0.0, 30.0, fs),
            out_gain: Smoothed::new(1.0, 20.0, fs),
            wet: Smoothed::new(0.0, 18.0, fs),
            breath_env: 0.0,
            breath_env_att: time_coeff(6.0, fs),
            breath_env_rel: time_coeff(60.0, fs),
            breath_hp: Biquad::highpass(fs, 3500.0, 0.7),
            noise: Noise::new(0x9E37_79B9),
            growl_phase: 0.0,
            pre_sat_hp: Biquad::highpass(fs, 70.0, 0.7),
            bass: Biquad::identity(),
            mud: Biquad::identity(),
            presence: Biquad::identity(),
            air: Biquad::identity(),
            reverb: Reverb::new(fs),
            comp: Compressor::new(fs),
            limiter: Limiter::new(fs),
            in_peak: 0.0,
            out_peak: 0.0,
            meter_decay: time_coeff(300.0, fs),
        };
        e.apply_settings(&s, true);
        e
    }

    pub fn sample_rate(&self) -> f32 {
        self.fs
    }

    pub fn settings(&self) -> &Settings {
        &self.s
    }

    /// Liga/desliga a voz processada (com crossfade curto, sem estalos).
    pub fn set_active(&mut self, active: bool) {
        self.wet.set(if active { 1.0 } else { 0.0 });
    }

    pub fn set_settings(&mut self, s: &Settings) {
        self.apply_settings(s, false);
    }

    fn apply_settings(&mut self, s: &Settings, snap: bool) {
        let s = s.sanitized();
        let fs = self.fs;

        self.gate.threshold_db = s.gate_threshold_db;
        self.in_gain.set(db_to_lin(s.input_gain_db));
        self.dry.set(s.dry_level);
        self.main.set(s.main_level);
        self.sub.set(s.sub_level);
        self.ghost.set(s.ghost_level);
        self.breath.set(s.breath_level);
        self.growl_depth.set(s.growl_depth);
        self.drive.set(s.drive);
        self.reverb_mix.set(s.reverb_mix);
        self.out_gain.set(db_to_lin(s.output_gain_db));

        for (ps, st) in [
            (&mut self.ps_main, s.main_semitones),
            (&mut self.ps_sub, s.sub_semitones),
            (&mut self.ps_ghost, s.ghost_semitones),
        ] {
            ps.set_grain(s.grain_ms, fs);
            if snap {
                ps.snap_semitones(st, fs);
            } else {
                ps.set_semitones(st);
            }
        }

        self.bass.set_from(&Biquad::low_shelf(fs, 150.0, s.bass_db));
        self.mud
            .set_from(&Biquad::peak(fs, 380.0, 1.0, s.mud_cut_db));
        self.presence
            .set_from(&Biquad::peak(fs, 2800.0, 0.9, s.presence_db));
        self.air.set_from(&Biquad::high_shelf(fs, 7000.0, s.air_db));
        self.reverb
            .set_params(s.reverb_size, s.reverb_damp, s.reverb_predelay_ms);

        self.comp.threshold_db = s.comp_threshold_db;
        self.comp.ratio = s.comp_ratio;
        // Sem makeup automático: com o sinal abaixo do limiar ele só amplificaria ruído.
        // O volume final é ajustado em `output_gain_db`.
        self.comp.makeup_db = 0.0;

        if snap {
            self.in_gain = Smoothed::new(db_to_lin(s.input_gain_db), 20.0, fs);
            self.dry = Smoothed::new(s.dry_level, 20.0, fs);
            self.main = Smoothed::new(s.main_level, 20.0, fs);
            self.sub = Smoothed::new(s.sub_level, 20.0, fs);
            self.ghost = Smoothed::new(s.ghost_level, 20.0, fs);
            self.breath = Smoothed::new(s.breath_level, 30.0, fs);
            self.growl_depth = Smoothed::new(s.growl_depth, 30.0, fs);
            self.drive = Smoothed::new(s.drive, 30.0, fs);
            self.reverb_mix = Smoothed::new(s.reverb_mix, 30.0, fs);
            self.out_gain = Smoothed::new(db_to_lin(s.output_gain_db), 20.0, fs);
        }
        self.s = s;
    }

    /// Latência aproximada da cadeia processada, em milissegundos.
    pub fn latency_ms(&self) -> f32 {
        let samples = self.ps_main.latency_samples();
        samples / self.fs * 1000.0
    }

    pub fn meters(&self) -> Meters {
        Meters {
            input_peak: self.in_peak,
            output_peak: self.out_peak,
            gain_reduction_db: self.comp.gain_reduction_db(),
        }
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let x = input * self.in_gain.tick();
        let bypass = x;

        let xg = self.gate.process(x);

        // --- camadas de voz ---
        if let Some(period) = self.tracker.process(xg) {
            self.ps_main.set_period(period);
            self.ps_sub.set_period(period);
            self.ps_ghost.set_period(period);
        }
        let main = self.ps_main.process(xg);
        let sub = self.ps_sub.process(xg);
        let ghost = self.ps_ghost.process(xg);

        // sopro: ruído de alta frequência seguindo o envelope da fala
        let a = xg.abs();
        let c = if a > self.breath_env {
            self.breath_env_att
        } else {
            self.breath_env_rel
        };
        self.breath_env = a + (self.breath_env - a) * c;
        let hiss = self.breath_hp.process(self.noise.tick());

        let mut v = xg * self.dry.tick()
            + main * self.main.tick()
            + sub * self.sub.tick()
            + ghost * self.ghost.tick()
            + hiss * (self.breath_env * 3.0).min(0.5) * self.breath.tick();

        // --- growl: AM rápida ---
        let depth = self.growl_depth.tick();
        if depth > 0.001 {
            self.growl_phase += self.s.growl_hz / self.fs;
            self.growl_phase -= self.growl_phase.floor();
            let lfo = 0.5 + 0.5 * (self.growl_phase * TAU).sin();
            v *= 1.0 - depth * 0.7 * lfo;
        }

        // --- saturação paralela ---
        let drive = self.drive.tick();
        if drive > 0.001 {
            let h = self.pre_sat_hp.process(v);
            let g = 1.0 + drive * 9.0;
            let sat = (g * h + 0.08).tanh() - 0.08f32.tanh();
            let sat = sat / g.sqrt();
            let m = (drive * 1.6).min(1.0);
            v = v * (1.0 - m) + sat * m * 1.8;
        }

        // --- EQ ---
        v = self.bass.process(v);
        v = self.mud.process(v);
        v = self.presence.process(v);
        v = self.air.process(v);

        // --- reverb ---
        v += self.reverb.process(v) * self.reverb_mix.tick();

        // --- dinâmica e ganho ---
        v = self.comp.process(v);
        v *= self.out_gain.tick();

        // --- bypass com crossfade ---
        let w = self.wet.tick();
        let out = self.limiter.process(v * w + bypass * (1.0 - w));

        // medidores (pico com decaimento)
        self.in_peak = (self.in_peak * self.meter_decay).max(input.abs());
        self.out_peak = (self.out_peak * self.meter_decay).max(out.abs());
        out
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        for (o, &i) in output.iter_mut().zip(input) {
            *o = self.process(i);
        }
    }
}
