//! Utilidades pequenas: conversões dB e suavizadores de parâmetros.

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

/// Coeficiente de filtro de 1 polo para uma constante de tempo em ms.
#[inline]
pub fn time_coeff(ms: f32, fs: f32) -> f32 {
    (-1.0 / (ms.max(0.01) * 0.001 * fs)).exp()
}

/// Suavizador de 1 polo para evitar "zipper noise" ao mexer em sliders.
#[derive(Clone, Copy, Debug)]
pub struct Smoothed {
    value: f32,
    target: f32,
    coeff: f32,
}

impl Smoothed {
    pub fn new(value: f32, ms: f32, fs: f32) -> Self {
        Self {
            value,
            target: value,
            coeff: time_coeff(ms, fs),
        }
    }

    #[inline]
    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.value = self.target + (self.value - self.target) * self.coeff;
        self.value
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}

/// Gerador de ruído branco barato (xorshift32), determinístico e sem alocação.
#[derive(Clone, Copy, Debug)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
