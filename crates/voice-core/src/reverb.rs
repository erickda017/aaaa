//! Reverb estilo Freeverb (8 combs + 4 allpass) em mono, com pré-delay.

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    filt: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            filt: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.idx];
        self.filt = out * (1.0 - damp) + self.filt * damp;
        self.buf[self.idx] = x + self.filt * feedback;
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.idx];
        let out = b - x;
        self.buf[self.idx] = x + b * 0.5;
        self.idx += 1;
        if self.idx == self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

pub struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    predelay: Vec<f32>,
    pd_idx: usize,
    pd_len: usize,
    feedback: f32,
    damp: f32,
    fs: f32,
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];

impl Reverb {
    pub fn new(fs: f32) -> Self {
        let scale = fs / 44_100.0;
        let sc = |n: usize| ((n as f32) * scale).round() as usize;
        let max_pd = (0.2 * fs) as usize + 1;
        let mut r = Self {
            combs: COMB_TUNING.iter().map(|&n| Comb::new(sc(n))).collect(),
            allpasses: ALLPASS_TUNING
                .iter()
                .map(|&n| Allpass::new(sc(n)))
                .collect(),
            predelay: vec![0.0; max_pd],
            pd_idx: 0,
            pd_len: 1,
            feedback: 0.8,
            damp: 0.4,
            fs,
        };
        r.set_params(0.5, 0.4, 10.0);
        r
    }

    /// `size` e `damp` em 0..1; `predelay_ms` em 0..200.
    pub fn set_params(&mut self, size: f32, damp: f32, predelay_ms: f32) {
        self.feedback = 0.70 + 0.28 * size.clamp(0.0, 1.0);
        self.damp = damp.clamp(0.0, 0.95);
        self.pd_len = ((predelay_ms.clamp(0.0, 200.0) * 0.001 * self.fs) as usize)
            .clamp(1, self.predelay.len());
    }

    /// Retorna apenas o sinal "molhado".
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // pré-delay
        let delayed = self.predelay[self.pd_idx];
        self.predelay[self.pd_idx] = x;
        self.pd_idx += 1;
        if self.pd_idx >= self.pd_len {
            self.pd_idx = 0;
        }

        let input = delayed * 0.015;
        let mut sum = 0.0;
        for c in &mut self.combs {
            sum += c.process(input, self.feedback, self.damp);
        }
        for a in &mut self.allpasses {
            sum = a.process(sum);
        }
        sum * 3.0
    }

    pub fn reset(&mut self) {
        for c in &mut self.combs {
            c.buf.fill(0.0);
            c.filt = 0.0;
        }
        for a in &mut self.allpasses {
            a.buf.fill(0.0);
        }
        self.predelay.fill(0.0);
    }
}
