//! Noise gate, compressor e limiter.

use crate::util::{db_to_lin, lin_to_db, time_coeff};

/// Gate com histerese e "hold" para não picotar o fim das palavras.
pub struct Gate {
    env: f32,
    env_rel: f32,
    gain: f32,
    gain_att: f32,
    gain_rel: f32,
    open: bool,
    hold: u32,
    hold_samples: u32,
    pub threshold_db: f32,
}

impl Gate {
    pub fn new(fs: f32) -> Self {
        Self {
            env: 0.0,
            env_rel: time_coeff(20.0, fs),
            gain: 1.0,
            gain_att: time_coeff(2.0, fs),
            gain_rel: time_coeff(80.0, fs),
            open: true,
            hold: 0,
            hold_samples: (0.12 * fs) as u32,
            threshold_db: -80.0,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        if self.threshold_db <= -79.0 {
            return x;
        }
        let a = x.abs();
        self.env = if a > self.env {
            a
        } else {
            a + (self.env - a) * self.env_rel
        };
        let db = lin_to_db(self.env);
        if db > self.threshold_db {
            self.open = true;
            self.hold = self.hold_samples;
        } else if db < self.threshold_db - 4.0 {
            if self.hold > 0 {
                self.hold -= 1;
            } else {
                self.open = false;
            }
        }
        let target = if self.open { 1.0 } else { 0.0 };
        let c = if target > self.gain {
            self.gain_att
        } else {
            self.gain_rel
        };
        self.gain = target + (self.gain - target) * c;
        x * self.gain
    }
}

/// Compressor feed-forward com joelho suave, detecção de pico no domínio dB.
pub struct Compressor {
    att: f32,
    rel: f32,
    gr_db: f32,
    pub threshold_db: f32,
    pub ratio: f32,
    pub makeup_db: f32,
}

const KNEE_DB: f32 = 6.0;

impl Compressor {
    pub fn new(fs: f32) -> Self {
        Self {
            att: time_coeff(4.0, fs),
            rel: time_coeff(140.0, fs),
            gr_db: 0.0,
            threshold_db: -18.0,
            ratio: 3.0,
            makeup_db: 0.0,
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x_db = lin_to_db(x.abs());
        let over = x_db - self.threshold_db;
        let out_db = if 2.0 * over < -KNEE_DB {
            x_db
        } else if 2.0 * over.abs() <= KNEE_DB {
            x_db + (1.0 / self.ratio - 1.0) * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        } else {
            self.threshold_db + over / self.ratio
        };
        let target = out_db - x_db; // <= 0
        let c = if target < self.gr_db {
            self.att
        } else {
            self.rel
        };
        self.gr_db = target + (self.gr_db - target) * c;
        x * db_to_lin(self.gr_db + self.makeup_db)
    }

    /// Redução de ganho atual em dB (valor positivo).
    pub fn gain_reduction_db(&self) -> f32 {
        -self.gr_db
    }
}

/// Limiter de ataque instantâneo + clamp final: a saída nunca passa de `ceiling`.
pub struct Limiter {
    gain: f32,
    rel: f32,
    pub ceiling: f32,
}

impl Limiter {
    pub fn new(fs: f32) -> Self {
        Self {
            gain: 1.0,
            rel: time_coeff(60.0, fs),
            ceiling: db_to_lin(-1.0),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let a = x.abs();
        let target = if a * self.gain > self.ceiling {
            self.ceiling / a
        } else {
            1.0
        };
        if target < self.gain {
            self.gain = target;
        } else {
            self.gain = 1.0 + (self.gain - 1.0) * self.rel;
        }
        (x * self.gain).clamp(-self.ceiling, self.ceiling)
    }
}
