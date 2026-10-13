use std::f32::consts::TAU;
use voice_core::period::PeriodTracker;
use voice_core::pitch::PitchShifter;
use voice_core::util::Noise;
use voice_core::{Engine, Settings, builtin_presets};

const FS: f32 = 48_000.0;

fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
    (0..(FS * secs) as usize)
        .map(|n| amp * (TAU * freq * n as f32 / FS).sin())
        .collect()
}

/// Frequência dominante: varre a magnitude da DFT em passos de 0,25 Hz perto do alvo
/// (cruzamentos de zero enganam com o crossfade do shifter, a DFT não).
fn peak_freq(x: &[f32], around: f32) -> f32 {
    let tail = &x[x.len() / 2..];
    let (mut best_f, mut best_m) = (0.0, 0.0f64);
    let mut f = around * 0.75;
    while f < around * 1.25 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, &s) in tail.iter().enumerate() {
            let w = 0.5 - 0.5 * (TAU as f64 * n as f64 / tail.len() as f64).cos();
            let ph = TAU as f64 * f as f64 * n as f64 / FS as f64;
            re += s as f64 * w * ph.cos();
            im += s as f64 * w * ph.sin();
        }
        let m = re * re + im * im;
        if m > best_m {
            best_m = m;
            best_f = f;
        }
        f += 0.25;
    }
    best_f
}

/// Roda shifter (+ detector de período, opcional) como o `Engine` faz.
fn run_shifter(input: &[f32], semi: f32, sync: bool) -> Vec<f32> {
    let mut ps = PitchShifter::new(FS, 32.0);
    ps.snap_semitones(semi, FS);
    let mut tr = PeriodTracker::new(FS);
    input
        .iter()
        .map(|&s| {
            if let Some(p) = tr.process(s)
                && sync
            {
                ps.set_period(p);
            }
            ps.process(s)
        })
        .collect()
}

/// Vogal sintética: 25 harmônicos com queda de 12 dB/oitava (como a fonte glotal), f0 deslizando.
fn vowel(f0_start: f32, f0_end: f32, secs: f32) -> Vec<f32> {
    let n = (FS * secs) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let f0 = f0_start + (f0_end - f0_start) * i as f32 / n as f32;
            phase += f0 / FS;
            phase -= phase.floor();
            (1..=25)
                .map(|h| (TAU * h as f32 * phase).sin() / (h * h) as f32)
                .sum::<f32>()
                * 0.5
        })
        .collect()
}

/// Oscilação do envelope RMS em janelas de 30 ms (maiores que um período de voz) (desvio padrão / média, trecho central).
fn envelope_flutter(x: &[f32]) -> f32 {
    let w = (FS * 0.030) as usize;
    let seg = &x[x.len() / 4..x.len() * 3 / 4];
    let env: Vec<f32> = seg
        .chunks(w)
        .filter(|c| c.len() == w)
        .map(|c| (c.iter().map(|v| v * v).sum::<f32>() / w as f32).sqrt())
        .collect();
    let mean = env.iter().sum::<f32>() / env.len() as f32;
    let var = env.iter().map(|e| (e - mean).powi(2)).sum::<f32>() / env.len() as f32;
    var.sqrt() / mean
}

#[test]
fn pitch_shifter_hits_target_frequency() {
    for (semi, expect) in [
        (-12.0f32, 110.0f32),
        (-7.0, 146.83),
        (-3.0, 185.0),
        (5.0, 293.66),
    ] {
        let out = run_shifter(&sine(220.0, 0.5, 3.0), semi, true);
        let f = peak_freq(&out, expect);
        let err = (f - expect).abs() / expect;
        assert!(
            err < 0.03,
            "shift {semi}: esperado {expect} Hz, medido {f} Hz"
        );
    }
}

#[test]
fn pitch_shifter_zero_semitones_is_a_pure_delay() {
    let mut ps = PitchShifter::new(FS, 32.0);
    let input = sine(300.0, 0.5, 1.0);
    let out: Vec<f32> = input.iter().map(|&s| ps.process(s)).collect();
    // Energia preservada (crossfade de Hann soma 1 para sinais coerentes).
    let rms = |v: &[f32]| (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt();
    let r = rms(&out[FS as usize / 2..]) / rms(&input[FS as usize / 2..]);
    assert!((r - 1.0).abs() < 0.03, "ganho {r}");
}

#[test]
fn bypass_is_transparent() {
    let mut e = Engine::new(FS);
    e.set_settings(&Settings::diabo());
    e.set_active(false);
    let mut n = Noise::new(7);
    let input: Vec<f32> = (0..48_000).map(|_| n.tick() * 0.3).collect();
    let mut max_diff = 0.0f32;
    for (i, &x) in input.iter().enumerate() {
        let y = e.process(x);
        // pula o início: o ganho de entrada ainda está se acomodando
        if i > 9_600 {
            max_diff = max_diff.max((y - x).abs());
        }
    }
    assert!(max_diff < 1e-3, "diferença {max_diff}");
}

#[test]
fn output_is_bounded_and_finite_for_all_presets() {
    let mut presets: Vec<_> = builtin_presets().into_iter().map(|(_, s)| s).collect();
    presets.push(Settings::neutral());
    for s in presets {
        let mut e = Engine::new(FS);
        e.set_settings(&s);
        e.set_active(true);
        let mut n = Noise::new(99);
        // ruído alto e senoide clipada, os piores casos
        for i in 0..(FS as usize * 2) {
            let x = if i < FS as usize {
                n.tick() * 2.0
            } else {
                (TAU * 150.0 * i as f32 / FS).sin() * 3.0
            };
            let y = e.process(x);
            assert!(y.is_finite(), "NaN/inf");
            assert!(y.abs() <= 0.8914, "estourou o teto: {y}");
        }
    }
}

#[test]
fn survives_garbage_settings() {
    let mut s = Settings::diabo();
    s.main_semitones = f32::NAN;
    s.reverb_size = f32::INFINITY;
    s.comp_ratio = 0.0;
    s.grain_ms = -5.0;
    s.drive = 1e9;
    let mut e = Engine::new(FS);
    e.set_settings(&s);
    e.set_active(true);
    let mut n = Noise::new(5);
    for _ in 0..FS as usize {
        let y = e.process(n.tick() * 0.5);
        assert!(y.is_finite() && y.abs() <= 0.8914);
    }
}

#[test]
fn toggling_has_no_big_discontinuities() {
    let mut e = Engine::new(FS);
    e.set_settings(&Settings::diabo());
    let input = sine(140.0, 0.3, 4.0);
    let mut prev = 0.0f32;
    let mut max_step = 0.0f32;
    for (i, &x) in input.iter().enumerate() {
        // alterna a cada meio segundo
        e.set_active((i / 24_000) % 2 == 1);
        let y = e.process(x);
        if i > 2_000 {
            max_step = max_step.max((y - prev).abs());
        }
        prev = y;
    }
    // uma senoide de 140 Hz a 0.3 tem passo máximo ~0.005; a voz processada é bem
    // mais energética, mas um clique de verdade passaria de 0.5 com folga.
    assert!(max_step < 0.35, "salto de {max_step} ao alternar");
}

#[test]
fn works_at_other_sample_rates() {
    for fs in [16_000.0f32, 44_100.0, 96_000.0] {
        let mut e = Engine::new(fs);
        e.set_settings(&Settings::abissal());
        e.set_active(true);
        let mut n = Noise::new(3);
        for _ in 0..fs as usize {
            let y = e.process(n.tick() * 0.4);
            assert!(y.is_finite() && y.abs() <= 0.8914, "fs {fs}");
        }
    }
}

#[test]
fn latency_is_reasonable() {
    let e = Engine::new(FS);
    assert!(
        e.latency_ms() < 25.0,
        "latência estimada {} ms",
        e.latency_ms()
    );
}

#[test]
fn period_sync_reduces_tremolo_on_voiced_speech() {
    let input = vowel(105.0, 135.0, 4.0);
    let plain = envelope_flutter(&run_shifter(&input, -7.0, false));
    let synced = envelope_flutter(&run_shifter(&input, -7.0, true));
    println!("flutter sem sync = {plain:.3}, com sync = {synced:.3}");
    assert!(
        synced < plain * 0.6,
        "sync deveria reduzir o tremolo: {plain} -> {synced}"
    );
    assert!(synced < 0.12, "tremolo ainda alto: {synced}");
}

#[test]
fn period_tracker_finds_pitch() {
    for f0 in [90.0f32, 120.0, 180.0, 250.0] {
        let mut tr = PeriodTracker::new(FS);
        let mut last = None;
        for s in vowel(f0, f0, 1.0) {
            if let Some(p) = tr.process(s) {
                last = Some(p);
            }
        }
        let p = last.expect("não detectou pitch");
        let est = FS / p;
        assert!((est - f0).abs() / f0 < 0.03, "f0 {f0}: detectado {est}");
    }
}
