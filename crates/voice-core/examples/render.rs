//! Aplica um preset a um arquivo WAV (ou a uma voz sintética) e grava o resultado.
//! Útil para comparar presets sem microfone e para checar níveis/pitch.
//!
//!   cargo run --release -p voice-core --example render -- <preset|todos> [entrada.wav|-] [pasta_saida]
//!
//! Presets: diabo, abissal, sussurro, sombrio, neutro. Sem WAV de entrada usa uma
//! "voz" sintética (vogais com pitch deslizando), só para teste.

use std::f32::consts::TAU;
use voice_core::biquad::Biquad;
use voice_core::period::PeriodTracker;
use voice_core::util::lin_to_db;
use voice_core::{Engine, Settings};

fn synth_voice(fs: f32) -> Vec<f32> {
    let secs = 6.0;
    let n = (fs * secs) as usize;
    let mut phase = 0.0f32;
    let mut formants = [
        Biquad::peak(fs, 700.0, 4.0, 14.0),
        Biquad::peak(fs, 1200.0, 5.0, 12.0),
        Biquad::peak(fs, 2600.0, 6.0, 9.0),
    ];
    (0..n)
        .map(|i| {
            let t = i as f32 / fs;
            // contorno de entonação: 125 → 150 → 112 Hz
            let f0 = 125.0 + 25.0 * (t * 1.1).sin() - 8.0 * (t * 0.37).cos();
            phase += f0 / fs;
            phase -= phase.floor();
            let src: f32 = (1..=30)
                .map(|h| (TAU * h as f32 * phase).sin() / (h * h) as f32)
                .sum();
            let mut v = src;
            for f in &mut formants {
                v = f.process(v);
            }
            // "sílabas": 0,35 s de voz, 0,15 s de pausa
            let env = if (t % 0.5) < 0.35 {
                (1.0 - ((t % 0.5) / 0.35 * 2.0 - 1.0).powi(8)).max(0.0)
            } else {
                0.0
            };
            v * env * 0.12
        })
        .collect()
}

fn read_wav(path: &str) -> (Vec<f32>, f32) {
    let mut r = hound::WavReader::open(path).expect("não consegui abrir o WAV");
    let spec = r.spec();
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.unwrap() as f32 / scale)
                .collect()
        }
    };
    let mono = samples
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    (mono, spec.sample_rate as f32)
}

fn median_f0(x: &[f32], fs: f32) -> f32 {
    let mut tr = PeriodTracker::new(fs);
    let mut v: Vec<f32> = x
        .iter()
        .filter_map(|&s| tr.process(s))
        .map(|p| fs / p)
        .collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn write_wav(path: &str, x: &[f32], fs: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: fs as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("não consegui criar o WAV");
    for s in x {
        w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .unwrap();
    }
    w.finalize().unwrap();
}

fn stats(x: &[f32]) -> (f32, f32) {
    let peak = x.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    let rms = (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
    (lin_to_db(peak), lin_to_db(rms))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let which = args.first().map(String::as_str).unwrap_or("todos");
    let (input, fs) = match args.get(1).filter(|p| !p.is_empty() && p.as_str() != "-") {
        Some(p) => read_wav(p),
        None => (synth_voice(48_000.0), 48_000.0),
    };
    let out_dir = args.get(2).cloned().unwrap_or_else(|| ".".into());

    let all = [
        ("neutro", Settings::neutral()),
        ("diabo", Settings::diabo()),
        ("abissal", Settings::abissal()),
        ("sussurro", Settings::sussurro()),
        ("sombrio", Settings::sombrio()),
    ];
    write_wav(&format!("{out_dir}/entrada.wav"), &input, fs);
    let (pk, rms) = stats(&input);
    println!(
        "entrada: {:.1} s @ {fs} Hz, pico {pk:.1} dBFS, RMS {rms:.1} dBFS, f0 mediano {:.0} Hz",
        input.len() as f32 / fs,
        median_f0(&input, fs)
    );

    for (name, s) in all.iter().filter(|(n, _)| which == "todos" || *n == which) {
        let mut e = Engine::new(fs);
        e.set_settings(s);
        e.set_active(true);
        let mut out: Vec<f32> = Vec::with_capacity(input.len());
        // 0,5 s de cauda extra para o reverb terminar
        let tail = (fs * 0.5) as usize;
        for &x in input.iter().chain(std::iter::repeat_n(&0.0, tail)) {
            out.push(e.process(x));
        }
        let (pk, rms) = stats(&out);
        println!(
            "{name:>9}: pico {pk:6.1} dBFS, RMS {rms:6.1} dBFS, f0 mediano {:4.0} Hz, latência do efeito ~{:.0} ms",
            median_f0(&out, fs),
            e.latency_ms()
        );

        write_wav(&format!("{out_dir}/{name}.wav"), &out, fs);
    }
}
