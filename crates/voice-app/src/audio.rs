//! Camada de áudio: microfone → [Engine] → cabo virtual (e, opcionalmente, fones).
//!
//! Dois relógios independentes (entrada e saída) nunca andam exatamente juntos, e as
//! placas podem usar taxas diferentes (44,1 x 48 kHz). Por isso o áudio processado
//! passa por um ring buffer e a saída lê com **reamostragem linear adaptativa**: a razão
//! de leitura é ajustada em até ±0,3% conforme o buffer enche ou esvazia, o que absorve
//! tanto a diferença de taxa quanto a deriva dos relógios sem estalos.

use crate::shared::Shared;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, DeviceId, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use voice_core::Engine;

#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
}

pub struct DeviceLists {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

fn describe(d: &Device) -> Option<DeviceInfo> {
    let id = d.id().ok()?.to_string();
    let name = d
        .description()
        .map(|x| x.name().to_string())
        .unwrap_or_else(|_| id.clone());
    Some(DeviceInfo { id, name })
}

pub fn list_devices() -> DeviceLists {
    let host = cpal::default_host();
    let collect = |it: Result<Vec<Device>, cpal::Error>| -> Vec<DeviceInfo> {
        it.map(|v| v.iter().filter_map(describe).collect())
            .unwrap_or_default()
    };
    DeviceLists {
        inputs: collect(host.input_devices().map(|i| i.collect())),
        outputs: collect(host.output_devices().map(|i| i.collect())),
        default_input: host
            .default_input_device()
            .and_then(|d| d.id().ok())
            .map(|i| i.to_string()),
        default_output: host
            .default_output_device()
            .and_then(|d| d.id().ok())
            .map(|i| i.to_string()),
    }
}

/// Procura o cabo de áudio virtual instalado (VB-Cable / Voicemeeter), se houver.
pub fn find_virtual_cable(outputs: &[DeviceInfo]) -> Option<&DeviceInfo> {
    let n = |d: &&DeviceInfo| d.name.to_lowercase();
    outputs
        .iter()
        .find(|d| n(d).contains("cable input"))
        .or_else(|| {
            outputs
                .iter()
                .find(|d| n(d).contains("voicemeeter") && n(d).contains("input"))
        })
}

fn find_device(id: &str, want_input: bool) -> Result<Device, String> {
    let host = cpal::default_host();
    let parsed: DeviceId = id
        .parse()
        .map_err(|_| format!("ID de dispositivo inválido: {id}"))?;
    let dev = host.device_by_id(&parsed).ok_or_else(|| {
        let kind = if want_input { "entrada" } else { "saída" };
        format!("Dispositivo de {kind} não encontrado (desconectado?). Atualize a lista.")
    })?;
    Ok(dev)
}

pub struct StartConfig {
    pub input_id: String,
    pub output_id: String,
    pub monitor_id: Option<String>,
    pub buffer_ms: f32,
}

pub struct RunInfo {
    pub in_rate: u32,
    pub out_rate: u32,
    pub engine_latency_ms: f32,
    pub buffer_ms: f32,
    pub monitor: bool,
}

/// Mantém os streams vivos; ao ser descartado, o áudio para.
pub struct Running {
    _input: Stream,
    _output: Stream,
    _monitor: Option<Stream>,
    pub info: RunInfo,
}

// ---------------------------------------------------------------------------------------
// Entrada: captura → mono → Engine → ring(s)
// ---------------------------------------------------------------------------------------

struct InputState {
    engine: Engine,
    shared: Arc<Shared>,
    main: HeapProd<f32>,
    monitor: Option<HeapProd<f32>>,
    channels: usize,
    settings_version: u64,
    active: bool,
}

impl InputState {
    fn on_data<T>(&mut self, data: &[T])
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        // 1) sincroniza parâmetros sem nunca bloquear a thread de áudio
        let v = self.shared.settings_version.load(Relaxed);
        if v != self.settings_version
            && let Ok(g) = self.shared.settings.try_lock()
        {
            self.engine.set_settings(&g);
            self.settings_version = v;
        }
        let active = self.shared.active.load(Relaxed);
        if active != self.active {
            self.engine.set_active(active);
            self.active = active;
        }
        let mon_vol = self.shared.monitor_volume();

        // 2) processa quadro a quadro
        let inv = 1.0 / self.channels as f32;
        let mut dropped = 0u64;
        for frame in data.chunks_exact(self.channels) {
            let mono = frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() * inv;
            let y = self.engine.process(mono);
            if self.main.try_push(y).is_err() {
                dropped += 1;
            }
            if let Some(m) = self.monitor.as_mut() {
                let _ = m.try_push(y * mon_vol);
            }
        }
        if dropped > 0 {
            self.shared.overruns.fetch_add(dropped, Relaxed);
        }
        self.shared.publish_meters(self.engine.meters());
    }
}

// ---------------------------------------------------------------------------------------
// Saída: ring → reamostrador adaptativo → dispositivo
// ---------------------------------------------------------------------------------------

struct Sink {
    cons: HeapCons<f32>,
    shared: Arc<Shared>,
    in_rate: f32,
    out_rate: f32,
    channels: usize,
    /// Quantas amostras (na taxa de entrada) queremos manter no buffer.
    target: usize,
    prev: f32,
    next: f32,
    pos: f32,
    primed: bool,
    fill_avg: f32,
}

impl Sink {
    fn new(
        cons: HeapCons<f32>,
        shared: Arc<Shared>,
        in_rate: u32,
        out_rate: u32,
        channels: usize,
        buffer_ms: f32,
    ) -> Self {
        let target = ((buffer_ms * 0.001 * in_rate as f32) as usize).max(64);
        Self {
            cons,
            shared,
            in_rate: in_rate as f32,
            out_rate: out_rate as f32,
            channels,
            target,
            prev: 0.0,
            next: 0.0,
            pos: 1.0,
            primed: false,
            fill_avg: target as f32,
        }
    }

    fn fill<T>(&mut self, data: &mut [T])
    where
        T: SizedSample + FromSample<f32>,
    {
        let occupied = self.cons.occupied_len();

        // Pré-enche antes de começar (e de novo após uma falta de dados) para o buffer
        // ter folga contra a variação do tamanho dos callbacks.
        if !self.primed {
            if occupied >= self.target {
                self.primed = true;
                self.fill_avg = occupied as f32;
            } else {
                data.fill(T::EQUILIBRIUM);
                return;
            }
        }
        if occupied == 0 {
            self.primed = false;
            self.shared.underruns.fetch_add(1, Relaxed);
            data.fill(T::EQUILIBRIUM);
            return;
        }
        // Rajada acumulada (ex.: a saída ficou parada): descarta o excesso.
        if occupied > self.target * 4 {
            self.cons.skip(occupied - self.target);
            self.shared.overruns.fetch_add(1, Relaxed);
        }

        // Controle proporcional da razão de leitura: buffer cheio demais → lê mais rápido.
        let occ_now = self.cons.occupied_len() as f32;
        self.fill_avg += (occ_now - self.fill_avg) * 0.1;
        let err = (self.fill_avg - self.target as f32) / self.target as f32;
        let corr = 1.0 + (err * 0.01).clamp(-0.003, 0.003);
        let step = self.in_rate / self.out_rate * corr;

        for frame in data.chunks_exact_mut(self.channels) {
            while self.pos >= 1.0 {
                self.prev = self.next;
                self.next = match self.cons.try_pop() {
                    Some(v) => v,
                    None => self.next * 0.95, // acabou no meio do bloco: decai a zero, sem "tom travado"
                };
                self.pos -= 1.0;
            }
            let s = self.prev + (self.next - self.prev) * self.pos;
            self.pos += step;
            let out = T::from_sample(s);
            for c in frame {
                *c = out;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Montagem dos streams
// ---------------------------------------------------------------------------------------

fn err_callback(
    shared: Arc<Shared>,
    what: &'static str,
) -> impl FnMut(cpal::Error) + Send + 'static {
    move |e: cpal::Error| {
        use cpal::ErrorKind::*;
        match e.kind() {
            Xrun => {
                shared.underruns.fetch_add(1, Relaxed);
            }
            DeviceChanged => {}
            _ => shared.set_error(format!("Erro de áudio ({what}): {e}")),
        }
    }
}

fn build_input<T>(
    device: &Device,
    config: StreamConfig,
    mut st: InputState,
    err: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device.build_input_stream(config, move |data: &[T], _| st.on_data(data), err, None)
}

fn build_output<T>(
    device: &Device,
    config: StreamConfig,
    mut sink: Sink,
    err: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    device.build_output_stream(config, move |data: &mut [T], _| sink.fill(data), err, None)
}

macro_rules! by_format {
    ($fmt:expr, $func:ident, $($arg:expr),+) => {
        match $fmt {
            SampleFormat::F32 => $func::<f32>($($arg),+),
            SampleFormat::I16 => $func::<i16>($($arg),+),
            SampleFormat::I32 => $func::<i32>($($arg),+),
            SampleFormat::U16 => $func::<u16>($($arg),+),
            SampleFormat::F64 => $func::<f64>($($arg),+),
            other => return Err(format!("Formato de amostra não suportado: {other}")),
        }
    };
}

pub fn start(shared: Arc<Shared>, cfg: &StartConfig) -> Result<Running, String> {
    let e = |what: &str| {
        let what = what.to_string();
        move |err: cpal::Error| format!("{what}: {err}")
    };

    let in_dev = find_device(&cfg.input_id, true)?;
    let out_dev = find_device(&cfg.output_id, false)?;
    if cfg.input_id == cfg.output_id {
        return Err("Entrada e saída são o mesmo dispositivo (causaria microfonia).".into());
    }

    let in_sup = in_dev
        .default_input_config()
        .map_err(e("Configuração do microfone"))?;
    let out_sup = out_dev
        .default_output_config()
        .map_err(e("Configuração da saída"))?;
    let (in_fmt, out_fmt) = (in_sup.sample_format(), out_sup.sample_format());
    let in_cfg: StreamConfig = in_sup.into();
    let out_cfg: StreamConfig = out_sup.into();

    let in_rate = in_cfg.sample_rate;
    let engine = Engine::new(in_rate as f32);
    let engine_latency_ms = engine.latency_ms();

    // Um segundo de folga por ring; o controle de nível mantém o uso real perto de `buffer_ms`.
    let ring_cap = in_rate as usize;
    let (main_prod, main_cons) = HeapRb::<f32>::new(ring_cap).split();

    let mut monitor_prod = None;
    let mut monitor_stream_parts = None;
    if let Some(mid) = &cfg.monitor_id {
        if mid == &cfg.input_id {
            return Err("O monitor não pode ser o mesmo dispositivo do microfone.".into());
        }
        let mon_dev = find_device(mid, false)?;
        let sup = mon_dev
            .default_output_config()
            .map_err(e("Configuração do monitor"))?;
        let fmt = sup.sample_format();
        let mcfg: StreamConfig = sup.into();
        let (p, c) = HeapRb::<f32>::new(ring_cap).split();
        monitor_prod = Some(p);
        monitor_stream_parts = Some((mon_dev, fmt, mcfg, c));
    }

    let state = InputState {
        engine,
        shared: shared.clone(),
        main: main_prod,
        monitor: monitor_prod,
        channels: in_cfg.channels as usize,
        settings_version: 0,
        active: false,
    };
    let sink = Sink::new(
        main_cons,
        shared.clone(),
        in_rate,
        out_cfg.sample_rate,
        out_cfg.channels as usize,
        cfg.buffer_ms,
    );

    let input = by_format!(
        in_fmt,
        build_input,
        &in_dev,
        in_cfg,
        state,
        err_callback(shared.clone(), "microfone")
    )
    .map_err(e("Não foi possível abrir o microfone"))?;
    let output = by_format!(
        out_fmt,
        build_output,
        &out_dev,
        out_cfg,
        sink,
        err_callback(shared.clone(), "saída")
    )
    .map_err(e("Não foi possível abrir a saída"))?;

    let mut monitor = None;
    if let Some((dev, fmt, mcfg, cons)) = monitor_stream_parts {
        let msink = Sink::new(
            cons,
            shared.clone(),
            in_rate,
            mcfg.sample_rate,
            mcfg.channels as usize,
            cfg.buffer_ms,
        );
        let s = by_format!(
            fmt,
            build_output,
            &dev,
            mcfg,
            msink,
            err_callback(shared.clone(), "monitor")
        )
        .map_err(e("Não foi possível abrir o monitor"))?;
        monitor = Some(s);
    }

    shared.underruns.store(0, Relaxed);
    shared.overruns.store(0, Relaxed);
    let _ = shared.take_error();

    // Saída e monitor primeiro (ficam esperando o buffer encher); entrada por último.
    output.play().map_err(e("Falha ao iniciar a saída"))?;
    if let Some(m) = &monitor {
        m.play().map_err(e("Falha ao iniciar o monitor"))?;
    }
    input.play().map_err(e("Falha ao iniciar o microfone"))?;

    Ok(Running {
        _input: input,
        _output: output,
        _monitor: monitor,
        info: RunInfo {
            in_rate,
            out_rate: out_cfg.sample_rate,
            engine_latency_ms,
            buffer_ms: cfg.buffer_ms,
            monitor: cfg.monitor_id.is_some(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use voice_core::Settings;

    /// Simula dois relógios independentes: o produtor entrega `in_rate` Hz, o consumidor
    /// pede em `out_rate * (1 + drift)` Hz, ambos em ticks de 10 ms.
    fn simulate(
        in_rate: u32,
        out_rate: u32,
        drift: f64,
        buffer_ms: f32,
        secs: f32,
    ) -> (Vec<f32>, Arc<Shared>, usize, usize) {
        let shared = Shared::new(Settings::neutral(), 0x77, false, 1.0);
        let (mut prod, cons) = HeapRb::<f32>::new(in_rate as usize).split();
        let channels = 2;
        let mut sink = Sink::new(cons, shared.clone(), in_rate, out_rate, channels, buffer_ms);
        let target = sink.target;

        let ticks = (secs * 100.0) as usize;
        let (mut n_in, mut out_acc) = (0usize, 0.0f64);
        let mut out = Vec::new();
        let mut fill_sum = 0usize;
        for t in 0..ticks {
            // produtor: 10 ms de seno de 440 Hz
            for _ in 0..(in_rate / 100) {
                let s = 0.5 * (TAU * 440.0 * n_in as f32 / in_rate as f32).sin();
                let _ = prod.try_push(s);
                n_in += 1;
            }
            // consumidor: 10 ms no relógio dele (com deriva), quadros fracionários acumulados
            out_acc += out_rate as f64 * (1.0 + drift) / 100.0;
            let frames = out_acc as usize;
            out_acc -= frames as f64;
            let mut buf = vec![0.0f32; frames * channels];
            sink.fill(&mut buf);
            out.extend(buf.chunks_exact(channels).map(|f| f[0]));
            if t > ticks / 2 {
                fill_sum += sink.cons.occupied_len();
            }
        }
        let avg_fill = fill_sum / (ticks - ticks / 2 - 1);
        (out, shared, target, avg_fill)
    }

    fn peak_freq(x: &[f32], fs: f32, around: f32) -> f32 {
        let (mut best, mut best_m) = (0.0, 0.0f64);
        let mut f = around - 5.0;
        while f < around + 5.0 {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (n, &s) in x.iter().enumerate() {
                let w = 0.5 - 0.5 * (TAU as f64 * n as f64 / x.len() as f64).cos();
                let ph = TAU as f64 * f as f64 * n as f64 / fs as f64;
                re += s as f64 * w * ph.cos();
                im += s as f64 * w * ph.sin();
            }
            let m = re * re + im * im;
            if m > best_m {
                best_m = m;
                best = f;
            }
            f += 0.1;
        }
        best
    }

    fn check(in_rate: u32, out_rate: u32, drift: f64) {
        let (out, shared, target, avg_fill) = simulate(in_rate, out_rate, drift, 35.0, 60.0);
        let (u, o) = (
            shared.underruns.load(Relaxed),
            shared.overruns.load(Relaxed),
        );
        // o priming inicial pode contar no máximo uma falta; depois disso nada deve estourar
        assert!(u <= 1, "{in_rate}->{out_rate} drift {drift}: {u} underruns");
        assert_eq!(o, 0, "{in_rate}->{out_rate} drift {drift}: {o} overruns");
        let err = (avg_fill as f32 - target as f32).abs() / target as f32;
        assert!(
            err < 0.35,
            "{in_rate}->{out_rate} drift {drift}: buffer médio {avg_fill} vs alvo {target}"
        );

        // pitch preservado nos últimos 2 s (a deriva real de 0,05% é inaudível: < 1 cent)
        let tail = &out[out.len() - 2 * out_rate as usize..];
        let f = peak_freq(tail, out_rate as f32, 440.0);
        assert!(
            (f - 440.0).abs() < 1.5,
            "{in_rate}->{out_rate} drift {drift}: pitch {f} Hz"
        );
        // sem saltos bruscos (estalos): um seno de 440 Hz a 0.5 muda no máx. ~0.03 por amostra
        let max_step = tail
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_step < 0.08,
            "{in_rate}->{out_rate} drift {drift}: salto de {max_step}"
        );
    }

    #[test]
    fn same_rate_with_drift() {
        check(48_000, 48_000, 0.0);
        check(48_000, 48_000, 0.0005);
        check(48_000, 48_000, -0.0005);
    }

    #[test]
    fn different_rates_with_drift() {
        check(48_000, 44_100, 0.0005);
        check(44_100, 48_000, -0.0005);
        check(48_000, 96_000, 0.0);
    }

    #[test]
    fn recovers_after_starvation() {
        let shared = Shared::new(Settings::neutral(), 0x77, false, 1.0);
        let (mut prod, cons) = HeapRb::<f32>::new(48_000).split();
        let mut sink = Sink::new(cons, shared.clone(), 48_000, 48_000, 2, 30.0);
        let mut buf = vec![0.0f32; 960];
        // sem dados: silêncio, sem pânico
        sink.fill(&mut buf);
        assert!(buf.iter().all(|&v| v == 0.0));
        // chegam dados: precisa voltar a tocar depois de encher o colchão
        for _ in 0..4800 {
            let _ = prod.try_push(0.25);
        }
        sink.fill(&mut buf);
        assert!(
            buf.iter().any(|&v| v != 0.0),
            "não retomou após receber dados"
        );
    }
}
