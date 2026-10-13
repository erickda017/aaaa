//! Filtros biquad (RBJ Audio EQ Cookbook), forma transposta direta II em f64.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn identity() -> Self {
        Self { b0: 1.0, ..Default::default() }
    }

    fn from_coeffs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    /// Troca os coeficientes preservando o estado interno (sem estalo).
    pub fn set_from(&mut self, other: &Biquad) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
    }

    fn trig(fs: f32, f: f32) -> (f64, f64) {
        let f = (f as f64).clamp(10.0, fs as f64 * 0.49);
        let w0 = 2.0 * PI * f / fs as f64;
        (w0.cos(), w0.sin())
    }

    pub fn highpass(fs: f32, f: f32, q: f32) -> Self {
        let (c, s) = Self::trig(fs, f);
        let alpha = s / (2.0 * q as f64);
        Self::from_coeffs((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn lowpass(fs: f32, f: f32, q: f32) -> Self {
        let (c, s) = Self::trig(fs, f);
        let alpha = s / (2.0 * q as f64);
        Self::from_coeffs((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn peak(fs: f32, f: f32, q: f32, gain_db: f32) -> Self {
        let (c, s) = Self::trig(fs, f);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let alpha = s / (2.0 * q as f64);
        Self::from_coeffs(
            1.0 + alpha * a,
            -2.0 * c,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * c,
            1.0 - alpha / a,
        )
    }

    pub fn low_shelf(fs: f32, f: f32, gain_db: f32) -> Self {
        let (c, s) = Self::trig(fs, f);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let beta = 2.0 * a.sqrt() * (s / 2.0 * 2f64.sqrt());
        Self::from_coeffs(
            a * ((a + 1.0) - (a - 1.0) * c + beta),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - beta),
            (a + 1.0) + (a - 1.0) * c + beta,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - beta,
        )
    }

    pub fn high_shelf(fs: f32, f: f32, gain_db: f32) -> Self {
        let (c, s) = Self::trig(fs, f);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let beta = 2.0 * a.sqrt() * (s / 2.0 * 2f64.sqrt());
        Self::from_coeffs(
            a * ((a + 1.0) + (a - 1.0) * c + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - beta),
            (a + 1.0) - (a - 1.0) * c + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - beta,
        )
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y as f32
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}
