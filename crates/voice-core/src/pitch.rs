//! Pitch shifter de domínio de tempo (duas leituras com crossfade de Hann sobre uma
//! linha de atraso variável).
//!
//! Vantagens: latência baixa (~metade do tamanho do grão) e custo mínimo, ideal para
//! tempo real. Como o espectro inteiro é reescalado, os formantes descem junto com o
//! pitch — exatamente o que dá a sensação de um "trato vocal maior" numa voz grave.

use crate::util::Smoothed;

pub struct PitchShifter {
    buf: Vec<f32>,
    mask: i64,
    write: i64,
    phase: f32,
    /// Tamanho de grão em uso (desliza suavemente até `grain_goal`).
    grain: f32,
    /// Tamanho de grão pedido pelo usuário.
    grain_user: f32,
    /// Grão-alvo já alinhado ao período da voz.
    grain_goal: f32,
    period: Option<f32>,
    semitones: Smoothed,
}

/// O grão muda no máximo isto por amostra (desvio de pitch <= 2% durante a transição).
const GRAIN_SLEW: f32 = 0.02;

const MIN_DELAY: f32 = 2.0;

impl PitchShifter {
    pub fn new(fs: f32, grain_ms: f32) -> Self {
        // Buffer cobre grãos de até ~150 ms em qualquer taxa de amostragem.
        let max_grain = (0.150 * fs) as usize + 16;
        let len = max_grain.next_power_of_two();
        let mut ps = Self {
            buf: vec![0.0; len],
            mask: len as i64 - 1,
            write: 0,
            phase: 0.0,
            grain: 0.0,
            grain_user: 0.0,
            grain_goal: 0.0,
            period: None,
            semitones: Smoothed::new(0.0, 25.0, fs),
        };
        ps.set_grain(grain_ms, fs);
        ps
    }

    fn max_grain(&self) -> f32 {
        (self.buf.len() - 8) as f32 - MIN_DELAY
    }

    pub fn set_grain(&mut self, grain_ms: f32, fs: f32) {
        let first = self.grain_user == 0.0;
        self.grain_user = (grain_ms * 0.001 * fs).clamp(64.0, self.max_grain());
        self.retarget();
        if first {
            self.grain = self.grain_goal;
        }
    }

    /// Informa o período fundamental da fala (em amostras). Os dois pontos de leitura
    /// passam a ficar separados por um número inteiro de períodos, ou seja, sempre em
    /// fase, o que elimina o cancelamento (tremolo) do crossfade em sons com pitch.
    pub fn set_period(&mut self, period: f32) {
        if period.is_finite() && period > 8.0 {
            self.period = Some(period);
            self.retarget();
        }
    }

    fn retarget(&mut self) {
        self.grain_goal = match self.period {
            Some(t) => {
                let k = (self.grain_user * 0.5 / t).round().max(1.0);
                (2.0 * k * t).clamp(64.0, self.max_grain())
            }
            None => self.grain_user,
        };
    }

    pub fn set_semitones(&mut self, st: f32) {
        self.semitones.set(st);
    }

    /// Salta direto para o valor alvo (usado na inicialização).
    pub fn snap_semitones(&mut self, st: f32, fs: f32) {
        self.semitones = Smoothed::new(st, 25.0, fs);
    }

    /// Atraso médio introduzido (em amostras).
    pub fn latency_samples(&self) -> f32 {
        MIN_DELAY + self.grain * 0.5
    }

    #[inline]
    fn read(&self, delay: f32) -> f32 {
        let pos = self.write as f32 - delay;
        let i = pos.floor();
        let t = pos - i;
        let i = i as i64;
        let y0 = self.buf[((i - 1) & self.mask) as usize];
        let y1 = self.buf[(i & self.mask) as usize];
        let y2 = self.buf[((i + 1) & self.mask) as usize];
        let y3 = self.buf[((i + 2) & self.mask) as usize];
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * t + c2) * t + c1) * t + y1
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.write += 1;
        self.buf[(self.write & self.mask) as usize] = x;

        let ratio = (self.semitones.tick() / 12.0).exp2();
        self.grain += (self.grain_goal - self.grain).clamp(-GRAIN_SLEW, GRAIN_SLEW);

        let p1 = self.phase;
        let p2 = if p1 >= 0.5 { p1 - 0.5 } else { p1 + 0.5 };
        let s1 = (std::f32::consts::PI * p1).sin();
        let s2 = (std::f32::consts::PI * p2).sin();
        let y = s1 * s1 * self.read(MIN_DELAY + p1 * self.grain)
            + s2 * s2 * self.read(MIN_DELAY + p2 * self.grain);

        // Atraso varia (1 - ratio) amostras por amostra.
        self.phase += (1.0 - ratio) / self.grain;
        self.phase -= self.phase.floor();
        y
    }

    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.phase = 0.0;
    }
}
