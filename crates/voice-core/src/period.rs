//! Detector de período fundamental (algoritmo YIN) barato o bastante para tempo real.
//!
//! Roda sobre o sinal decimado para ~12 kHz e produz uma estimativa a cada ~5 ms.
//! O pitch shifter usa o período para alinhar seus pontos de leitura (ver `pitch.rs`).

pub struct PeriodTracker {
    decim: usize,
    acc: f32,
    acc_n: usize,
    ring: Vec<f32>,
    pos: usize,
    filled: usize,
    since_hop: usize,
    hop: usize,
    win: usize,
    tau_min: usize,
    tau_max: usize,
    scratch: Vec<f32>,
    cmnd: Vec<f32>,
}

const THRESHOLD: f32 = 0.15;

impl PeriodTracker {
    pub fn new(fs: f32) -> Self {
        let decim = ((fs / 12_000.0).round() as usize).max(1);
        let fs_dec = fs / decim as f32;
        let tau_min = ((fs_dec / 500.0) as usize).max(2); // f0 máximo 500 Hz
        let tau_max = (fs_dec / 65.0) as usize; // f0 mínimo 65 Hz
        let win = tau_max + 64;
        let n = win + tau_max + 1;
        Self {
            decim,
            acc: 0.0,
            acc_n: 0,
            ring: vec![0.0; n],
            pos: 0,
            filled: 0,
            since_hop: 0,
            hop: ((0.005 * fs_dec) as usize).max(16),
            win,
            tau_min,
            tau_max,
            scratch: vec![0.0; n],
            cmnd: vec![0.0; tau_max + 2],
        }
    }

    /// Alimenta uma amostra. Retorna `Some(período em amostras da taxa original)`
    /// quando uma nova estimativa confiável (voz com pitch) está disponível.
    #[inline]
    pub fn process(&mut self, x: f32) -> Option<f32> {
        self.acc += x;
        self.acc_n += 1;
        if self.acc_n < self.decim {
            return None;
        }
        let v = self.acc / self.decim as f32;
        self.acc = 0.0;
        self.acc_n = 0;

        self.ring[self.pos] = v;
        self.pos += 1;
        if self.pos == self.ring.len() {
            self.pos = 0;
        }
        self.filled = (self.filled + 1).min(self.ring.len());
        self.since_hop += 1;
        if self.since_hop >= self.hop && self.filled == self.ring.len() {
            self.since_hop = 0;
            return self.estimate();
        }
        None
    }

    fn estimate(&mut self) -> Option<f32> {
        // linearizar o ring (mais antigo primeiro)
        let n = self.ring.len();
        for i in 0..n {
            self.scratch[i] = self.ring[(self.pos + i) % n];
        }
        // O trecho analisado é o mais recente: x[off..off+win] contra x[off+τ..].
        let off = n - self.win - self.tau_max - 1;
        let x = &self.scratch[off..];

        let energy: f32 = x[..self.win].iter().map(|v| v * v).sum::<f32>() / self.win as f32;
        if energy < 1e-7 {
            return None;
        }

        let mut running = 0.0f32;
        self.cmnd[0] = 1.0;
        for tau in 1..=self.tau_max {
            let mut d = 0.0f32;
            for j in 0..self.win {
                let diff = x[j] - x[j + tau];
                d += diff * diff;
            }
            running += d;
            self.cmnd[tau] = if running > 0.0 {
                d * tau as f32 / running
            } else {
                1.0
            };
        }

        let mut tau = self.tau_min;
        while tau < self.tau_max {
            if self.cmnd[tau] < THRESHOLD {
                while tau + 1 < self.tau_max && self.cmnd[tau + 1] < self.cmnd[tau] {
                    tau += 1;
                }
                // interpolação parabólica
                let (a, b, c) = (self.cmnd[tau - 1], self.cmnd[tau], self.cmnd[tau + 1]);
                let denom = a - 2.0 * b + c;
                let shift = if denom.abs() > 1e-9 {
                    0.5 * (a - c) / denom
                } else {
                    0.0
                };
                return Some((tau as f32 + shift.clamp(-1.0, 1.0)) * self.decim as f32);
            }
            tau += 1;
        }
        None
    }
}
