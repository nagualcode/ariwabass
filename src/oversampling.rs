//! Linear-phase 4x oversampling built from two cascaded Kaiser-windowed half-band FIR stages.
//!
//! Used to run the Akai MFC42 `1SS133` silicon diode / `NJM4558M` post-filter growl stage
//! and the dbx 166 `PeakStop` ceiling clipper free of aliasing foldback.
//! Every other tap of a half-band filter (`N = 4K + 3`) is identically zero and the
//! centre tap is `0.5`, so each polyphase branch reduces to a compact dot product or
//! a pure integer delay. All buffers are pre-allocated at construction time (`zero`
//! allocations on the audio thread).

use std::f64::consts::PI;

/// Stage 1 (1x <-> 2x): 63 taps, ~92 dB stopband, passband to ~19.5 kHz @ 44.1 kHz.
const STAGE1_TAPS: usize = 63;
/// Stage 2 (2x <-> 4x): 31 taps, ~92 dB stopband, passband well beyond base-rate Nyquist.
const STAGE2_TAPS: usize = 31;
/// Kaiser beta for >92 dB stopband attenuation.
const KAISER_BETA: f64 = 9.2;

/// Round-trip latency of [`Oversampler4x`] in base-rate samples.
///
/// Stage 1 contributes `(63 - 1) / 2 = 31` samples. Stage 2 contributes
/// `(31 - 1) / 4 = 7.5` samples, padded to 8 with a 2-sample delay at the 4x rate
/// so the total latency is an exact integer (`39` samples) for phase-coherent dry mixing.
pub const OVERSAMPLER_LATENCY: u32 = ((STAGE1_TAPS - 1) / 2 + (STAGE2_TAPS - 1) / 4 + 1) as u32;

/// Zeroth-order modified Bessel function of the first kind (power series).
fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half_x = x * 0.5;
    for k in 1..64 {
        let f = half_x / f64::from(k);
        term *= f * f;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

/// Design the non-zero ("even index") taps of a Kaiser-windowed half-band filter.
fn design_halfband(num_taps: usize) -> Vec<f32> {
    debug_assert!(num_taps % 4 == 3, "half-band length must be 4K + 3");
    let centre = (num_taps - 1) / 2;
    let denom = bessel_i0(KAISER_BETA);
    let mut taps: Vec<f64> = (0..num_taps)
        .step_by(2)
        .map(|j| {
            let n = j as f64 - centre as f64;
            let ideal = (PI * n * 0.5).sin() / (PI * n);
            let r = 2.0 * j as f64 / (num_taps - 1) as f64 - 1.0;
            let window = bessel_i0(KAISER_BETA * (1.0 - r * r).max(0.0).sqrt()) / denom;
            ideal * window
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    for t in &mut taps {
        *t *= 0.5 / sum;
    }
    taps.iter().map(|&t| t as f32).collect()
}

/// Dot product with four independent accumulators for auto-vectorization (NEON/AVX).
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0.0f32; 4];
    let (chunks_a, rem_a) = a.as_chunks::<4>();
    let (chunks_b, rem_b) = b.as_chunks::<4>();
    let tail: f32 = rem_a.iter().zip(rem_b).map(|(x, y)| x * y).sum();
    for (ca, cb) in chunks_a.iter().zip(chunks_b) {
        for k in 0..4 {
            acc[k] += ca[k] * cb[k];
        }
    }
    (acc[0] + acc[1]) + (acc[2] + acc[3]) + tail
}

/// Fixed-length history with mirrored backing store for contiguous slice access.
struct History {
    buf: Vec<f32>,
    len: usize,
    pos: usize,
}

impl History {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; 2 * len],
            len,
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = if self.pos == 0 {
            self.len - 1
        } else {
            self.pos - 1
        };
        self.buf[self.pos] = x;
        self.buf[self.pos + self.len] = x;
    }

    #[inline]
    fn slice(&self) -> &[f32] {
        &self.buf[self.pos..self.pos + self.len]
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }
}

struct HalfbandStage {
    taps: Vec<f32>,
    k: usize,
    even: History,
    odd: History,
}

impl HalfbandStage {
    fn new(num_taps: usize) -> Self {
        let taps = design_halfband(num_taps);
        let k = (num_taps - 3) / 4;
        let m = taps.len();
        Self {
            taps,
            k,
            even: History::new(m),
            odd: History::new(k + 2),
        }
    }

    #[inline]
    fn upsample(&mut self, x: f32) -> [f32; 2] {
        self.even.push(x);
        let hist = self.even.slice();
        let y0 = 2.0 * dot(&self.taps, hist);
        let y1 = hist[self.k];
        [y0, y1]
    }

    #[inline]
    fn downsample(&mut self, v: [f32; 2]) -> f32 {
        self.even.push(v[0]);
        let y = dot(&self.taps, self.even.slice()) + 0.5 * self.odd.slice()[self.k];
        self.odd.push(v[1]);
        y
    }

    fn clear(&mut self) {
        self.even.clear();
        self.odd.clear();
    }
}

/// 4x up -> nonlinearity -> 4x down per channel.
pub struct Oversampler4x {
    up1: HalfbandStage,
    up2: HalfbandStage,
    down2: HalfbandStage,
    down1: HalfbandStage,
    pad: [f32; 2],
}

impl Default for Oversampler4x {
    fn default() -> Self {
        Self {
            up1: HalfbandStage::new(STAGE1_TAPS),
            up2: HalfbandStage::new(STAGE2_TAPS),
            down2: HalfbandStage::new(STAGE2_TAPS),
            down1: HalfbandStage::new(STAGE1_TAPS),
            pad: [0.0; 2],
        }
    }
}

impl Oversampler4x {
    pub fn reset(&mut self) {
        self.up1.clear();
        self.up2.clear();
        self.down2.clear();
        self.down1.clear();
        self.pad = [0.0; 2];
    }

    /// Run `f` at 4x the base rate on one base-rate sample.
    #[inline]
    pub fn process(&mut self, x: f32, mut f: impl FnMut(f32) -> f32) -> f32 {
        let [a, b] = self.up1.upsample(x);
        let [s0, s1] = self.up2.upsample(a);
        let [s2, s3] = self.up2.upsample(b);
        let (s0, s1, s2, s3) = (f(s0), f(s1), f(s2), f(s3));
        let (p0, p1) = (self.pad[0], self.pad[1]);
        self.pad = [s2, s3];
        let a = self.down2.downsample([p0, p1]);
        let b = self.down2.downsample([s0, s1]);
        self.down1.downsample([a, b])
    }
}

/// Fixed-capacity delay line to phase-align dry signal with [`OVERSAMPLER_LATENCY`].
pub struct LatencyCompDelay {
    buf: [f32; 64],
    pos: usize,
}

impl Default for LatencyCompDelay {
    fn default() -> Self {
        Self {
            buf: [0.0; 64],
            pos: 0,
        }
    }
}

impl LatencyCompDelay {
    pub fn reset(&mut self) {
        self.buf = [0.0; 64];
        self.pos = 0;
    }

    #[inline]
    pub fn process(&mut self, x: f32, delay_samples: usize) -> f32 {
        if delay_samples == 0 {
            return x;
        }
        let d = delay_samples.min(63);
        self.buf[self.pos] = x;
        let read_idx = (self.pos + 64 - d) & 63;
        self.pos = (self.pos + 1) & 63;
        self.buf[read_idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halfband_taps_have_unity_dc() {
        for n in [STAGE1_TAPS, STAGE2_TAPS] {
            let taps = design_halfband(n);
            let sum: f32 = taps.iter().sum();
            assert!((sum - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn impulse_emerges_after_reported_latency() {
        let mut os = Oversampler4x::default();
        let mut out = Vec::new();
        for i in 0..128 {
            out.push(os.process(if i == 0 { 1.0 } else { 0.0 }, |s| s));
        }
        let peak = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .unwrap()
            .0;
        assert_eq!(peak as u32, OVERSAMPLER_LATENCY);
    }
}
