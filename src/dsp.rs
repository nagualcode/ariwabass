//! Component-level physical & analog synthesizer DSP model of Mad Professor's (Neil Fraser)
//! 1980s/1990s **Ariwa Sound Studios** "Ronca e Desliza" Dub Bass Instrument.
//!
//! ### Architecture Overview
//! 1. **Monophonic / Legato MIDI Bass Voice + Analog RC Portamento (`Glide`)**:
//!    - 16-note last-note-priority stack with true legato detection, pitch bend ($\pm 2$ semitones),
//!      mod wheel (`CC1` -> extra filter wobble/growl), and GUI keyboard audition support.
//!    - Exponential RC portamento circuit ($\tau_{\text{glide}} \in [0, 500\text{ ms}]$) that smoothly
//!      glides the fundamental frequency $f_0(t)$ between notes.
//! 2. **4 Classic Ariwa 1980s/1990s Bass Oscillator Models (`BassVoiceModel`)**:
//!    - **`SH-101 Sub`**: Band-limited PolyBLEP Sawtooth + PWM Pulse + phase-locked $-1\text{ Oct}$ Sub.
//!    - **`Mini 3x`**: Triple detuned PolyBLEP Minimoog Model D oscillator stack (`0`, `+7.5`, `-7.5` cents).
//!    - **`Flatwound`**: Velocity-sensitive physical model of a Fender Jazz/Precision bass with flatwound strings.
//!    - **`Steppers`**: 1990s UK Dub / Jah Shaka steppers bass (pure phase-locked Sine sub + stereo detuned Saws).
//! 3. **MXR Model 129 Companded Sub-Octave (`Sub Oct`)**:
//!    - Phase-locked $-1\text{ Octave}$ ($f_0 / 2$) sub-harmonic generator shaped through Tony Gambacurta's
//!      **`Am6072` $\mu$-255 differential compander** and 24 dB/oct reconstruction filter.
//! 4. **Switchable "Ronca e Desliza" Effect Engine (`ariwa_fx_on`)**:
//!    - **Key-Tracked Akai MFC42 (`5x SSM2164S` + `4x NJM2082M`) 2P / 4P / 8P State-Variable Filter**:
//!      Tracks the playing MIDI note frequency $f_0(t)$ so the resonant vocal growl (`Ronca`) stays
//!      harmonically locked to every note across the keyboard instead of droning at a fixed pitch.
//!    - **Mu-Tron III (US Patent 3,911,776) & Line 6 Filter Pro (`Program 41 LOW`, US Patent 5,789,689)
//!      Note-Triggered CdS Optocoupler Envelope (`Desliza`) + Groove LFO (`Wobble`)**.
//!    - **4x Oversampled `1SS133` Silicon Diode / Class-A Harmonic Growl Stage** ($H_2 > H_3$ dominance).
//!    - **Dual MXR Model 131 (M-110) 6-Stage Vactrol Auto Phaser (`Phaser`)** above a $95\text{ Hz}$ crossover.
//! 5. **Otari MTR-90 2" Tape Head Bump ($62\text{ Hz}$), Faraday $1/f$ Transformer Flux (`Drive`),
//!    and UREI LA-4 + dbx 166 True-RMS VCA & PeakStop® Limiter (`Glue`)**.

use std::f32::consts::{PI, TAU};
use truce::prelude::ParamEnum;

use crate::oversampling::{LatencyCompDelay, OVERSAMPLER_LATENCY, Oversampler4x};

/// Classic 1980s/1990s Ariwa Studios bass sound generators.
#[derive(ParamEnum)]
pub enum BassVoiceModel {
    /// Roland SH-101 / Juno-60 monophonic UK dub synth bass (PolyBLEP Saw + Pulse + Sub).
    #[name = "SH-101 Sub"]
    Sh101Sub,
    /// Moog Minimoog Model D triple detuned analog oscillator stack.
    #[name = "Mini 3x"]
    Minimoog3x,
    /// Plucked Fender Jazz/Precision flatwound electric bass physical model.
    #[name = "Flatwound"]
    FlatwoundJazz,
    /// 1990s UK Dub Steppers bass (pure phase-locked Sub Sine + stereo detuned Saws).
    #[name = "Steppers"]
    SteppersReese,
}

/// Selectable filter slope from the Akai MFC42 analog filter architecture.
#[derive(ParamEnum)]
pub enum FilterSlope {
    /// 1x 2-Pole SSM2164 SVF (-12 dB/octave, Oberheim SEM / Mu-Tron III style).
    #[name = "2P (12 dB/oct)"]
    TwoPole,
    /// 2x Cascaded 2-Pole SSM2164 SVFs (-24 dB/octave, Mutator / Classic 4-Pole).
    #[name = "4P (24 dB/oct)"]
    FourPole,
    /// 4x Cascaded 2-Pole SSM2164 SVFs (-48 dB/octave, Akai MFC42 Mono Brickwall Growl).
    #[name = "8P (48 dB/oct)"]
    EightPole,
}

impl FilterSlope {
    #[inline]
    pub fn stages(self) -> usize {
        match self {
            Self::TwoPole => 1,
            Self::FourPole => 2,
            Self::EightPole => 4,
        }
    }
}

/// Tempo-synced LFO division (Akai MFC42 Beat Module / Line 6 Filter Pro Note Matrix).
#[derive(ParamEnum)]
pub enum SyncDivision {
    #[name = "1/1 Bar"]
    Bar1,
    #[name = "1/2"]
    Half,
    #[name = "1/4"]
    Quarter,
    #[name = "1/4T"]
    QuarterTriplet,
    #[name = "1/8"]
    Eighth,
    #[name = "1/8D"]
    EighthDotted,
    #[name = "1/8T"]
    EighthTriplet,
    #[name = "1/16"]
    Sixteenth,
}

impl SyncDivision {
    #[inline]
    pub fn cycles_per_beat(self) -> f32 {
        match self {
            Self::Bar1 => 0.25,
            Self::Half => 0.5,
            Self::Quarter => 1.0,
            Self::QuarterTriplet => 1.5,
            Self::Eighth => 2.0,
            Self::EighthDotted => 4.0 / 3.0,
            Self::EighthTriplet => 3.0,
            Self::Sixteenth => 4.0,
        }
    }
}

/// Per-block discrete configuration settings.
#[derive(Clone, Copy)]
pub struct BlockConfig {
    /// Master switch for the psychedelic "Ronca e Desliza" effect section.
    /// When false, the instrument plays as a clean, deep Ariwa analog dub bass.
    pub ariwa_fx_on: bool,
    pub voice_model: BassVoiceModel,
    pub slope: FilterSlope,
    pub sync: bool,
    pub sync_div: SyncDivision,
    pub tempo_bpm: f32,
    pub hq: bool,
    /// Optional MIDI note triggered from the on-screen GUI keyboard (`0` = none, `1..=127` = active note).
    pub gui_note: u8,
}

/// Per-sample smoothed continuous controls.
#[derive(Clone, Copy)]
pub struct SampleControls {
    /// Base filter cutoff harmonic focus in Hz (35..2500 Hz, key-tracked to active MIDI note).
    pub cutoff_hz: f32,
    /// Coupled SSM2164 resonance + 1SS133 post-filter growl (0..1).
    pub ronca: f32,
    /// Bipolar Mu-Tron III / MFC42 envelope slide depth (-1..+1; <0 = Tron Down, >0 = Tron Up).
    pub desliza: f32,
    /// Groove LFO modulation depth (0..1).
    pub wobble: f32,
    /// LFO rate in Hz (0.08..12.0 Hz) and optical glide speed scaler.
    pub speed_hz: f32,
    /// Analog RC pitch portamento / legato slide time in milliseconds (0..500 ms).
    pub glide_ms: f32,
    /// MXR 129 sub-octave (-1 Oct) companded bass level (0..1).
    pub sub_oct: f32,
    /// Amplitude & filter pluck decay/sustain contour in milliseconds (80..2500 ms).
    pub decay_ms: f32,
    /// Otari 2" tape + MFC42 diode drive (0..1).
    pub drive: f32,
    /// MXR 131 6-stage Vactrol Auto Phaser depth/mix (0..1).
    pub phaser: f32,
    /// UREI LA-4 + dbx 166 compression & PeakStop amount (0..1).
    pub glue: f32,
    /// Linear output gain trim.
    pub out_gain: f32,
}

/// Telemetry returned per stereo frame for the Ableton Live UI meters & visualizers.
#[derive(Clone, Copy)]
pub struct FrameOut {
    pub left: f32,
    pub right: f32,
    /// Instantaneous modulated filter cutoff in Hz.
    pub active_cutoff_hz: f32,
    /// Instantaneous MXR 131 phaser center frequency in Hz.
    pub phaser_f0_hz: f32,
    /// Instantaneous optical envelope follower value (0..1).
    pub env: f32,
    /// Linear gain reduction multiplier (<= 1.0) per channel.
    pub gr_lin: [f32; 2],
    /// Currently sounding MIDI note (0 if silent, otherwise 1..=127).
    pub active_note: u8,
    /// Instantaneous fundamental pitch in Hz.
    pub note_hz: f32,
}

// ============================================================================
// 1. Biquad & Faraday 1/f Flux Weighting Filter Primitives
// ============================================================================

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn set_highpass(&mut self, fc: f32, q: f32, sr: f32) {
        let w0 = TAU * (fc / sr).clamp(1e-5, 0.45);
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let a0_inv = 1.0 / (1.0 + alpha);
        let half_1_cos = 0.5 * (1.0 + cos_w);
        self.b0 = half_1_cos * a0_inv;
        self.b1 = -(1.0 + cos_w) * a0_inv;
        self.b2 = half_1_cos * a0_inv;
        self.a1 = (-2.0 * cos_w) * a0_inv;
        self.a2 = (1.0 - alpha) * a0_inv;
    }

    fn set_lowpass(&mut self, fc: f32, q: f32, sr: f32) {
        let w0 = TAU * (fc / sr).clamp(1e-4, 0.45);
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let a0_inv = 1.0 / (1.0 + alpha);
        let half_1_cos = 0.5 * (1.0 - cos_w);
        self.b0 = half_1_cos * a0_inv;
        self.b1 = (1.0 - cos_w) * a0_inv;
        self.b2 = half_1_cos * a0_inv;
        self.a1 = (-2.0 * cos_w) * a0_inv;
        self.a2 = (1.0 - alpha) * a0_inv;
    }

    fn set_peaking(&mut self, fc: f32, q: f32, gain_db: f32, sr: f32) {
        let a = 10.0f32.powf(gain_db / 40.0);
        let w0 = TAU * (fc / sr).clamp(1e-4, 0.45);
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let a0_inv = 1.0 / (1.0 + alpha / a);
        self.b0 = (1.0 + alpha * a) * a0_inv;
        self.b1 = (-2.0 * cos_w) * a0_inv;
        self.b2 = (1.0 - alpha * a) * a0_inv;
        self.a1 = (-2.0 * cos_w) * a0_inv;
        self.a2 = (1.0 - alpha / a) * a0_inv;
    }
}

/// Matched 1st-order low-shelf emphasis / de-emphasis pair for Faraday $1/f$ transformer flux.
#[derive(Clone, Copy, Default)]
struct FluxWeightingPair {
    b0: f32,
    b1: f32,
    a1: f32,
    pre_z1: f32,
    post_z1: f32,
}

impl FluxWeightingPair {
    fn reset(&mut self) {
        self.pre_z1 = 0.0;
        self.post_z1 = 0.0;
    }

    fn update(&mut self, fc: f32, boost_db: f32, sr: f32) {
        let g = 10.0f32.powf(boost_db / 20.0);
        let k = (PI * (fc / sr).clamp(1e-4, 0.45)).tan();
        let norm = 1.0 / (1.0 + k);
        self.b0 = (1.0 + g * k) * norm;
        self.b1 = (g * k - 1.0) * norm;
        self.a1 = (k - 1.0) * norm;
    }

    #[inline]
    fn pre_emphasis(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.pre_z1;
        self.pre_z1 = self.b1 * x - self.a1 * y;
        y
    }

    #[inline]
    fn de_emphasis(&mut self, x: f32) -> f32 {
        let b0_inv = 1.0 / self.b0.max(1e-6);
        let y = b0_inv * (x + self.post_z1);
        self.post_z1 = self.a1 * x - self.b1 * y;
        y
    }
}

// ============================================================================
// 2. Band-Limited PolyBLEP Dub Bass Synthesizer Voice + MXR 129 Sub-Octave
// ============================================================================

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 1e-7 {
        return 0.0;
    }
    if t < dt {
        let u = t / dt;
        u + u - u * u - 1.0
    } else if t > 1.0 - dt {
        let u = (t - 1.0) / dt;
        u * u + u + u + 1.0
    } else {
        0.0
    }
}

#[inline]
fn polyblep_saw(phase: f32, dt: f32) -> f32 {
    (2.0 * phase - 1.0) - poly_blep(phase, dt)
}

#[inline]
fn polyblep_pulse(phase: f32, pw: f32, dt: f32) -> f32 {
    let raw = if phase < pw { 1.0 } else { -1.0 };
    let b1 = poly_blep(phase, dt);
    let b2 = poly_blep((phase - pw).rem_euclid(1.0), dt);
    raw + b1 - b2
}

/// `Am6072` $\mu$-255 8-bit companding DAC nonlinearity from the MXR 129 Pitch Transposer.
#[inline]
fn mu255_compand(x: f32) -> f32 {
    const MU: f32 = 255.0;
    const INV_LN_1_MU: f32 = 1.0 / 5.5451774;
    let s = x.clamp(-1.5, 1.5);
    let mag = s.abs();
    let compressed = (1.0 + MU * mag).ln() * INV_LN_1_MU;
    let quant = (compressed * 127.0).round() / 127.0;
    let expanded = ((quant / INV_LN_1_MU).exp() - 1.0) / MU;
    expanded.copysign(s)
}

/// Monophonic / Legato Dub Bass Voice with 16-note priority stack, RC portamento,
/// 4 oscillator models, MXR 129 companded sub-octave, and Mu-Tron III optical envelope.
struct AriwaSynthVoice {
    /// Held MIDI notes `[(note, velocity); 16]`
    note_stack: [(u8, u8); 16],
    stack_len: usize,
    last_gui_note: u8,
    /// Current target MIDI note (default 40 = E1, 41.2 Hz)
    target_note: u8,
    /// Gliding pitch in semitones (for analog RC portamento)
    current_semi: f32,
    has_played_once: bool,
    /// Pitch bend in semitones (-2..+2)
    pitch_bend_semi: f32,
    /// Mod wheel (CC1, 0..1)
    mod_wheel: f32,
    /// Sustain pedal (CC64)
    sustain_pedal: bool,
    sustained_note: Option<(u8, u8)>,
    /// Note velocity (0..1)
    velocity: f32,
    /// Amplitude VCA envelope (0..1) and stage
    amp_env: f32,
    decay_env: f32,
    /// Pluck transient burst for Flatwound electric bass model
    pluck_env: f32,
    /// Mu-Tron III CdS optocoupler filter envelope (fast + slow photoconductive memory)
    opto_fast: f32,
    opto_slow: f32,
    /// Transient 555 VCO pitch-glide envelope
    pitch_swoop_env: f32,
    /// Oscillator phases (0..1)
    phase_main: f32,
    phase_det_up: f32,
    phase_det_dn: f32,
    phase_sub: f32,
    pwm_phase: f32,
    /// Groove LFO phase & Vactrol smoother
    lfo_phase: f32,
    vactrol_lfo: f32,
    /// MXR 129 sub-octave reconstruction filter
    sub_recon_lpf: Biquad,
    /// Flatwound string body damping filter
    string_body_lpf: Biquad,
}

impl Default for AriwaSynthVoice {
    fn default() -> Self {
        let mut v = Self {
            note_stack: [(0, 0); 16],
            stack_len: 0,
            last_gui_note: 0,
            target_note: 40, // E1 (41.20 Hz)
            current_semi: 40.0,
            has_played_once: false,
            pitch_bend_semi: 0.0,
            mod_wheel: 0.0,
            sustain_pedal: false,
            sustained_note: None,
            velocity: 0.85,
            amp_env: 0.0,
            decay_env: 0.0,
            pluck_env: 0.0,
            opto_fast: 0.0,
            opto_slow: 0.0,
            pitch_swoop_env: 0.0,
            phase_main: 0.0,
            phase_det_up: 0.23,
            phase_det_dn: 0.61,
            phase_sub: 0.0,
            pwm_phase: 0.0,
            lfo_phase: 0.0,
            vactrol_lfo: 0.0,
            sub_recon_lpf: Biquad::default(),
            string_body_lpf: Biquad::default(),
        };
        v.set_sample_rate(44_100.0);
        v
    }
}

impl AriwaSynthVoice {
    fn set_sample_rate(&mut self, sr: f32) {
        self.sub_recon_lpf.set_lowpass(180.0, 0.80, sr);
        self.string_body_lpf.set_lowpass(950.0, 0.95, sr);
    }

    fn reset(&mut self) {
        self.note_stack = [(0, 0); 16];
        self.stack_len = 0;
        self.last_gui_note = 0;
        self.pitch_bend_semi = 0.0;
        self.mod_wheel = 0.0;
        self.sustain_pedal = false;
        self.sustained_note = None;
        self.amp_env = 0.0;
        self.decay_env = 0.0;
        self.pluck_env = 0.0;
        self.opto_fast = 0.0;
        self.opto_slow = 0.0;
        self.pitch_swoop_env = 0.0;
        self.sub_recon_lpf.reset();
        self.string_body_lpf.reset();
    }

    #[inline]
    fn is_gate_open(&self) -> bool {
        self.stack_len > 0 || self.sustained_note.is_some()
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        if velocity == 0 {
            self.note_off(note);
            return;
        }
        let was_open = self.is_gate_open();
        self.sustained_note = None;

        // Remove existing entry for `note` if present
        if let Some(pos) = self.note_stack[..self.stack_len]
            .iter()
            .position(|&(n, _)| n == note)
        {
            for i in pos..self.stack_len - 1 {
                self.note_stack[i] = self.note_stack[i + 1];
            }
            self.stack_len -= 1;
        }
        if self.stack_len < 16 {
            self.note_stack[self.stack_len] = (note, velocity);
            self.stack_len += 1;
        } else {
            self.note_stack[15] = (note, velocity);
        }

        self.target_note = note;
        let vel_norm = (f32::from(velocity) / 127.0).clamp(0.1, 1.0);
        self.velocity = 0.30 + 0.70 * vel_norm.powf(0.75);

        if !self.has_played_once {
            self.current_semi = f32::from(note);
            self.has_played_once = true;
        }

        if !was_open {
            // Fresh note trigger from silence: reset pluck, filter envelopes & LFO phase cleanly
            self.decay_env = 1.0;
            self.pluck_env = 1.0;
            self.pitch_swoop_env = 1.0;
            self.lfo_phase = 0.0;
            self.opto_fast = 0.0;
            self.opto_slow = 0.0;
        } else {
            // Legato note transition: re-excite filter & pluck gently while keeping portamento smooth
            self.decay_env = (self.decay_env + 0.65).min(1.0);
            self.pluck_env = 0.70;
            self.pitch_swoop_env = 0.60;
        }
    }

    pub fn note_off(&mut self, note: u8) {
        if let Some(pos) = self.note_stack[..self.stack_len]
            .iter()
            .position(|&(n, _)| n == note)
        {
            let removed = self.note_stack[pos];
            for i in pos..self.stack_len - 1 {
                self.note_stack[i] = self.note_stack[i + 1];
            }
            self.stack_len -= 1;

            if self.stack_len > 0 {
                // Legato return to the previously held note!
                let (prev_note, prev_vel) = self.note_stack[self.stack_len - 1];
                self.target_note = prev_note;
                let vel_norm = (f32::from(prev_vel) / 127.0).clamp(0.1, 1.0);
                self.velocity = 0.30 + 0.70 * vel_norm.powf(0.75);
            } else if self.sustain_pedal {
                self.sustained_note = Some(removed);
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.stack_len = 0;
        self.sustain_pedal = false;
        self.sustained_note = None;
    }

    pub fn set_gui_note(&mut self, gui_note: u8) {
        if gui_note == self.last_gui_note {
            return;
        }
        let prev = self.last_gui_note;
        self.last_gui_note = gui_note;
        if gui_note > 0 {
            self.note_on(gui_note, 108);
        }
        if prev > 0 {
            self.note_off(prev);
        }
    }

    /// Synthesizes one stereo oscillator + MXR 129 sub-octave frame and returns:
    /// `(osc_left, osc_right, fundamental_hz, opto_env_0_to_1, lfo_bipolar)`.
    #[inline]
    fn step(
        &mut self,
        model: BassVoiceModel,
        c: &SampleControls,
        lfo_rate_hz: f32,
        ariwa_fx_on: bool,
        sr: f32,
    ) -> (f32, f32, f32, f32, f32) {
        let gate = self.is_gate_open();

        // 1. Analog RC Pitch Portamento ("Desliza" no pitch!)
        let target_semi = f32::from(self.target_note);
        if c.glide_ms <= 1.0 {
            self.current_semi = target_semi;
        } else {
            let tau_glide = (c.glide_ms * 0.001).clamp(0.001, 0.800);
            let glide_c = (-1.0 / (tau_glide * sr)).exp();
            self.current_semi = glide_c * self.current_semi + (1.0 - glide_c) * target_semi;
        }

        // Transient 555 VCO pitch-swoop envelope (active when Desliza < 0 in Tron Down mode)
        let swoop_decay = (-1.0 / (0.075 * sr)).exp();
        self.pitch_swoop_env *= swoop_decay;
        let tron_down_swoop_semi = if ariwa_fx_on && c.desliza < -0.05 {
            (-c.desliza) * 2.5 * self.pitch_swoop_env
        } else {
            0.0
        };

        let effective_semi =
            self.current_semi + self.pitch_bend_semi + tron_down_swoop_semi;
        let note_hz = (440.0 * 2.0f32.powf((effective_semi - 69.0) / 12.0)).clamp(20.0, 4000.0);

        // 2. Amplitude VCA Envelope & Pluck Decay Contour (TB-303 / Minimoog Style)
        let decay_sec = (c.decay_ms * 0.001).clamp(0.06, 2.5);
        let dec_c = (-1.0 / (decay_sec * sr)).exp();
        self.decay_env *= dec_c;

        let pluck_c = (-1.0 / (0.015 * sr)).exp();
        self.pluck_env *= pluck_c;

        // Controlled bass sustain (prevents runaway drone while holding notes):
        // Short decays (80-350 ms) decay to near zero for tight percussive dub punch.
        // Long decays (800-2500 ms) settle to a quiet, warm sustain floor (max 0.25).
        let sustain_lvl = ((c.decay_ms - 150.0) / 2800.0).clamp(0.0, 0.25);
        let target_amp = if gate {
            self.velocity * (sustain_lvl + (1.0 - sustain_lvl) * self.decay_env)
        } else {
            0.0
        };

        let amp_tau = if gate {
            if target_amp > self.amp_env {
                0.0018 // 1.8 ms fast punchy analog transient attack
            } else {
                0.035
            }
        } else {
            // Tight 25 ms release: cuts cleanly without click or lingering resonant drone
            0.025
        };
        let amp_c = (-1.0 / (amp_tau * sr)).exp();
        self.amp_env = amp_c * self.amp_env + (1.0 - amp_c) * target_amp;

        // 3. Mu-Tron III CdS Optocoupler Filter Envelope (US Patent 3,911,776)
        let filt_env_pulse = self.decay_env * self.decay_env;
        let opto_drive = if gate {
            self.velocity * filt_env_pulse
        } else {
            0.0
        };
        let atk_c = (-1.0 / (0.0035 * sr)).exp();
        let rel1_c = (-1.0 / (0.075 * sr)).exp();
        let rel2_c = (-1.0 / (0.220 * sr)).exp();

        if opto_drive > self.opto_fast {
            self.opto_fast = atk_c * self.opto_fast + (1.0 - atk_c) * opto_drive;
        } else {
            self.opto_fast = rel1_c * self.opto_fast + (1.0 - rel1_c) * opto_drive;
        }
        if self.opto_fast > self.opto_slow {
            self.opto_slow = 0.95 * self.opto_slow + 0.05 * self.opto_fast;
        } else {
            self.opto_slow = rel2_c * self.opto_slow + (1.0 - rel2_c) * self.opto_fast;
        }
        let combined_i = 0.80 * self.opto_fast + 0.20 * self.opto_slow;
        let i_pow = combined_i.max(0.0).powf(0.92);
        let opto_env = (1.30 * i_pow / (0.30 + i_pow)).clamp(0.0, 1.0);

        // 4. Groove LFO with Vactrol Turnaround Smoothing
        self.lfo_phase = (self.lfo_phase + lfo_rate_hz / sr).fract();
        let tri = 1.0 - 4.0 * (self.lfo_phase - 0.5).abs();
        let sin = (TAU * self.lfo_phase).sin();
        let raw_lfo = 0.65 * tri + 0.35 * sin;
        let lfo_tau = if raw_lfo > self.vactrol_lfo {
            0.006
        } else {
            0.026
        };
        let lfo_c = (-1.0 / (lfo_tau * sr)).exp();
        self.vactrol_lfo = lfo_c * self.vactrol_lfo + (1.0 - lfo_c) * raw_lfo;

        if self.amp_env < 1e-5 {
            return (0.0, 0.0, note_hz, opto_env, self.vactrol_lfo);
        }

        // 5. Advance Phase-Locked Oscillators
        let dt_main = (note_hz / sr).clamp(1e-6, 0.45);
        let dt_up = (note_hz * 1.0043 / sr).clamp(1e-6, 0.45); // +7.5 cents
        let dt_dn = (note_hz * 0.9957 / sr).clamp(1e-6, 0.45); // -7.5 cents
        let dt_sub = 0.5 * dt_main; // Exact -1 Octave (f0 / 2)

        self.phase_main = (self.phase_main + dt_main).fract();
        self.phase_det_up = (self.phase_det_up + dt_up).fract();
        self.phase_det_dn = (self.phase_det_dn + dt_dn).fract();
        self.phase_sub = (self.phase_sub + dt_sub).fract();
        self.pwm_phase = (self.pwm_phase + 0.42 / sr).fract();

        let pw = 0.48 + 0.22 * (TAU * self.pwm_phase).sin();

        let (osc_l, osc_r) = match model {
            BassVoiceModel::Sh101Sub => {
                // Classic Roland SH-101: Sawtooth + PWM Pulse + fundamental sine body
                let saw = polyblep_saw(self.phase_main, dt_main);
                let pulse = polyblep_pulse(self.phase_main, pw, dt_main);
                let fund_sin = (TAU * self.phase_main).sin();
                let mono = 0.46 * saw + 0.34 * pulse + 0.30 * fund_sin;
                (mono, mono)
            }
            BassVoiceModel::Minimoog3x => {
                // Moog Minimoog Model D: 3 detuned oscillators (Saw + Detuned Saw + Detuned Pulse/Tri)
                let saw1 = polyblep_saw(self.phase_main, dt_main);
                let saw2 = polyblep_saw(self.phase_det_up, dt_up);
                let saw3 = polyblep_saw(self.phase_det_dn, dt_dn);
                let fund = (TAU * self.phase_main).sin();
                let l = 0.42 * saw1 + 0.34 * saw2 + 0.16 * saw3 + 0.25 * fund;
                let r = 0.42 * saw1 + 0.16 * saw2 + 0.34 * saw3 + 0.25 * fund;
                (l, r)
            }
            BassVoiceModel::FlatwoundJazz => {
                // Fender Jazz/Precision Bass with Flatwound Strings:
                // Warm fundamental + 2nd/3rd pickup comb + velocity finger-pluck transient
                let p = self.phase_main;
                let fund = (TAU * p).sin();
                let h2 = (2.0 * TAU * p + 0.3).sin() * (0.45 + 0.35 * self.decay_env);
                let h3 = (3.0 * TAU * p).sin() * (0.22 + 0.30 * self.decay_env);
                let bridge_comb = polyblep_pulse(p, 0.28, dt_main) * (0.20 + 0.55 * self.pluck_env);
                let raw_string = 0.62 * fund + 0.28 * h2 + 0.16 * h3 + bridge_comb;
                let body = self.string_body_lpf.process(raw_string);
                (body, body)
            }
            BassVoiceModel::SteppersReese => {
                // 90s UK Dub Steppers: Heavy fundamental Sine + stereo-detuned Reese Saws
                let sub_sin = (TAU * self.phase_main).sin();
                let saw_up = polyblep_saw(self.phase_det_up, dt_up);
                let saw_dn = polyblep_saw(self.phase_det_dn, dt_dn);
                let saw_mid = polyblep_saw(self.phase_main, dt_main);
                let l = 0.52 * sub_sin + 0.28 * saw_mid + 0.32 * saw_up;
                let r = 0.52 * sub_sin + 0.28 * saw_mid + 0.32 * saw_dn;
                (l, r)
            }
        };

        // 6. MXR Model 129 Sub-Octave (-1 Oct at f0 / 2 with Am6072 mu-255 companding)
        let sub_wave = if c.sub_oct > 1e-4 {
            let sub_sin = (TAU * self.phase_sub).sin();
            let sub_sq = polyblep_pulse(self.phase_sub, 0.5, dt_sub);
            let raw_sub = 0.72 * sub_sin + 0.28 * sub_sq;
            let companded = mu255_compand(raw_sub);
            self.sub_recon_lpf.process(companded) * c.sub_oct * 0.95
        } else {
            0.0
        };

        let env_gain = self.amp_env;
        let out_l = (osc_l + sub_wave) * env_gain;
        let out_r = (osc_r + sub_wave) * env_gain;

        (out_l, out_r, note_hz, opto_env, self.vactrol_lfo)
    }
}

// ============================================================================
// 3. Akai MFC42 Cascaded SSM2164S + NJM2082M 2P / 4P / 8P State-Variable Filter
// ============================================================================

#[derive(Clone, Copy, Default)]
struct Ssm2164SvfStage {
    ic1eq: f32,
    ic2eq: f32,
}

impl Ssm2164SvfStage {
    fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    #[inline]
    fn process_lp(
        &mut self,
        x: f32,
        g: f32,
        k: f32,
        a1: f32,
        a2: f32,
        a3: f32,
        res_drive: f32,
    ) -> f32 {
        let v3 = x - self.ic2eq;
        let v1_lin = a1 * self.ic1eq + a2 * v3;
        // Internal 1SS133 silicon diode soft-limiting across the bandpass resonator state
        let v1 = if res_drive > 0.01 {
            let s = v1_lin * (1.0 + 0.55 * res_drive);
            let asym = s - 0.08 * res_drive * (s * s) / (1.0 + s.abs());
            (asym / (1.0 + 0.35 * res_drive * asym * asym)) / (1.0 + 0.35 * res_drive)
        } else {
            v1_lin
        };
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        let _ = (g, k);
        v2
    }
}

#[derive(Clone, Copy, Default)]
struct Mfc42FilterChannel {
    stages: [Ssm2164SvfStage; 4],
}

impl Mfc42FilterChannel {
    fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }

    #[inline]
    fn process(
        &mut self,
        x: f32,
        cutoff_hz: f32,
        ronca: f32,
        slope: FilterSlope,
        sr: f32,
    ) -> f32 {
        let fc = cutoff_hz.clamp(28.0, (sr * 0.42).min(12_000.0));
        let g = (PI * fc / sr).tan();

        let n_stages = slope.stages();
        let total_q = 0.707 + ronca * ronca * 5.2;
        let stage_q = match slope {
            FilterSlope::TwoPole => total_q,
            FilterSlope::FourPole => total_q.powf(0.55).max(0.707),
            FilterSlope::EightPole => total_q.powf(0.35).max(0.707),
        };
        let k = 1.0 / stage_q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let comp = 1.0 / (1.0 + 0.25 * ronca);
        let mut sig = x * comp;
        for idx in 0..n_stages {
            let stage_drive = ronca * (0.65 + 0.35 * (idx as f32 / n_stages as f32));
            sig = self.stages[idx].process_lp(sig, g, k, a1, a2, a3, stage_drive);
        }
        sig
    }
}

/// Akai MFC42 post-filter `NJM4558M` + anti-parallel `1SS133` silicon diode harmonic growl stage.
#[inline]
pub fn mfc42_diode_growl(x: f32, ronca: f32, extra_drive: f32) -> f32 {
    let total_drive = (0.35 * ronca + 0.85 * extra_drive).clamp(0.0, 1.2);
    if total_drive < 1e-4 {
        return x;
    }
    let pregain = 1.0 + 1.35 * total_drive;
    let v = x * pregain;
    // Discrete Class-A / asymmetric 1SS133 diode transfer law (H2 > H3 dominance)
    let alpha_even = 0.38 * total_drive;
    let beta_odd = 0.065 * total_drive;
    let v_asym = v
        - alpha_even * (v * v) / (1.0 + v.abs())
        - beta_odd * (v * v * v) / (1.0 + v * v);

    let thresh = 1.15;
    let knee = 0.65;
    let mag = v_asym.abs();
    let clipped = if mag <= thresh {
        v_asym
    } else {
        (thresh + knee * ((mag - thresh) / knee).tanh()).copysign(v_asym)
    };

    let recovery = 1.0 / (1.0 + 1.18 * total_drive);
    clipped * recovery
}

// ============================================================================
// 4. MXR Model 131 (M-110) Dual 6-Stage Vactrol Auto Phaser
// ============================================================================

#[derive(Clone, Copy, Default)]
struct FirstOrderAllPass {
    z1: f32,
}

impl FirstOrderAllPass {
    fn reset(&mut self) {
        self.z1 = 0.0;
    }

    #[inline]
    fn process(&mut self, x: f32, a: f32) -> f32 {
        let y = -a * x + self.z1;
        self.z1 = x + a * y;
        y
    }
}

struct Mxr131AutoPhaser {
    ap_l: [FirstOrderAllPass; 6],
    ap_r: [FirstOrderAllPass; 6],
    fb_l: f32,
    fb_r: f32,
    vactrol_cv_l: f32,
    vactrol_cv_r: f32,
    phase: f32,
    xover_lp_l: f32,
    xover_lp_r: f32,
    last_f0_hz: f32,
}

impl Default for Mxr131AutoPhaser {
    fn default() -> Self {
        Self {
            ap_l: [FirstOrderAllPass::default(); 6],
            ap_r: [FirstOrderAllPass::default(); 6],
            fb_l: 0.0,
            fb_r: 0.0,
            vactrol_cv_l: 0.0,
            vactrol_cv_r: 0.0,
            phase: 0.0,
            xover_lp_l: 0.0,
            xover_lp_r: 0.0,
            last_f0_hz: 380.0,
        }
    }
}

impl Mxr131AutoPhaser {
    fn reset(&mut self) {
        for s in &mut self.ap_l {
            s.reset();
        }
        for s in &mut self.ap_r {
            s.reset();
        }
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.vactrol_cv_l = 0.0;
        self.vactrol_cv_r = 0.0;
        self.phase = 0.0;
        self.xover_lp_l = 0.0;
        self.xover_lp_r = 0.0;
        self.last_f0_hz = 380.0;
    }

    #[inline]
    fn process_stereo(
        &mut self,
        left: f32,
        right: f32,
        phaser_mix: f32,
        speed_hz: f32,
        env_mod: f32,
        sr: f32,
    ) -> (f32, f32, f32) {
        if phaser_mix < 1e-4 {
            return (left, right, self.last_f0_hz);
        }

        let ph_rate = (speed_hz * 0.35).clamp(0.06, 3.5);
        self.phase = (self.phase + ph_rate / sr).fract();

        let tri_l = 1.0 - 4.0 * (self.phase - 0.5).abs();
        let phase_r = (self.phase + 0.1667).fract();
        let tri_r = 1.0 - 4.0 * (phase_r - 0.5).abs();

        let target_l = (0.75 * tri_l + 0.25 * (env_mod * 2.0 - 1.0)).clamp(-1.0, 1.0);
        let target_r = (0.75 * tri_r + 0.25 * (env_mod * 2.0 - 1.0)).clamp(-1.0, 1.0);

        let tau_l = if target_l > self.vactrol_cv_l { 0.005 } else { 0.090 };
        let tau_r = if target_r > self.vactrol_cv_r { 0.005 } else { 0.090 };
        let c_l = (-1.0 / (tau_l * sr)).exp();
        let c_r = (-1.0 / (tau_r * sr)).exp();
        self.vactrol_cv_l = c_l * self.vactrol_cv_l + (1.0 - c_l) * target_l;
        self.vactrol_cv_r = c_r * self.vactrol_cv_r + (1.0 - c_r) * target_r;

        let f0_l = 380.0 * 2.0f32.powf(1.55 * self.vactrol_cv_l);
        let f0_r = 380.0 * 2.0f32.powf(1.55 * self.vactrol_cv_r);
        self.last_f0_hz = f0_l;

        let g_l = (PI * (f0_l / sr).clamp(0.001, 0.42)).tan();
        let a_l = (1.0 - g_l) / (1.0 + g_l);
        let g_r = (PI * (f0_r / sr).clamp(0.001, 0.42)).tan();
        let a_r = (1.0 - g_r) / (1.0 + g_r);

        // 95 Hz 1st-order phase-coherent crossover: sub-bass stays 100% mono-anchored
        let xover_g = (PI * 95.0 / sr).tan();
        let xover_c = xover_g / (1.0 + xover_g);
        self.xover_lp_l += xover_c * (left - self.xover_lp_l);
        self.xover_lp_r += xover_c * (right - self.xover_lp_r);
        let sub_mono = 0.5 * (self.xover_lp_l + self.xover_lp_r);
        let high_l = left - self.xover_lp_l;
        let high_r = right - self.xover_lp_r;

        let regen = 0.14 * phaser_mix;
        let mut s_l = (high_l + regen * self.fb_l).clamp(-3.0, 3.0);
        let mut s_r = (high_r + regen * self.fb_r).clamp(-3.0, 3.0);
        for stage in &mut self.ap_l {
            s_l = stage.process(s_l, a_l);
        }
        for stage in &mut self.ap_r {
            s_r = stage.process(s_r, a_r);
        }
        self.fb_l = s_l.tanh();
        self.fb_r = s_r.tanh();

        let depth = 0.45 * phaser_mix;
        let out_l = sub_mono + (1.0 - 0.28 * depth) * high_l + depth * s_l;
        let out_r = sub_mono + (1.0 - 0.28 * depth) * high_r + depth * s_r;
        (out_l, out_r, f0_l)
    }
}

// ============================================================================
// 5. UREI LA-4 Optical Compressor + dbx 166 Blackmer True-RMS VCA & PeakStop
// ============================================================================

struct AriwaDynamicsGlue {
    rms_sq: f32,
    opto_gr_db: f32,
    slow_afterglow_db: f32,
    slew_prev_l: f32,
    slew_prev_r: f32,
}

impl Default for AriwaDynamicsGlue {
    fn default() -> Self {
        Self {
            rms_sq: 0.0,
            opto_gr_db: 0.0,
            slow_afterglow_db: 0.0,
            slew_prev_l: 0.0,
            slew_prev_r: 0.0,
        }
    }
}

impl AriwaDynamicsGlue {
    fn reset(&mut self) {
        self.rms_sq = 0.0;
        self.opto_gr_db = 0.0;
        self.slow_afterglow_db = 0.0;
        self.slew_prev_l = 0.0;
        self.slew_prev_r = 0.0;
    }

    #[inline]
    fn process_stereo(&mut self, left: f32, right: f32, glue: f32, sr: f32) -> (f32, f32, f32) {
        if glue < 1e-4 {
            self.slew_prev_l = left;
            self.slew_prev_r = right;
            return (left, right, 1.0);
        }

        let power = 0.5 * (left * left + right * right);
        let rms_c = (-1.0 / (0.018 * sr)).exp();
        self.rms_sq = rms_c * self.rms_sq + (1.0 - rms_c) * power;
        let rms_db = if self.rms_sq > 1e-12 {
            10.0 * self.rms_sq.log10()
        } else {
            -120.0
        };

        let thresh_db = -10.0 - 14.0 * glue;
        let ratio = 2.5 + 5.5 * glue;
        let knee_w = 8.0;
        let over = rms_db - thresh_db;
        let target_gr_db = if over <= -0.5 * knee_w {
            0.0
        } else if over < 0.5 * knee_w {
            let t = over + 0.5 * knee_w;
            (1.0 - 1.0 / ratio) * (t * t) / (2.0 * knee_w)
        } else {
            (1.0 - 1.0 / ratio) * over
        };

        let atk_c = (-1.0 / (0.006 * sr)).exp();
        let rel1_c = (-1.0 / (0.065 * sr)).exp();
        let rel2_c = (-1.0 / (0.650 * sr)).exp();

        if target_gr_db > self.opto_gr_db {
            self.opto_gr_db = atk_c * self.opto_gr_db + (1.0 - atk_c) * target_gr_db;
        } else {
            let floor = 0.32 * self.slow_afterglow_db;
            let goal = target_gr_db.max(floor);
            self.opto_gr_db = rel1_c * self.opto_gr_db + (1.0 - rel1_c) * goal;
        }
        if self.opto_gr_db > self.slow_afterglow_db {
            self.slow_afterglow_db = 0.992 * self.slow_afterglow_db + 0.008 * self.opto_gr_db;
        } else {
            self.slow_afterglow_db = rel2_c * self.slow_afterglow_db;
        }

        let makeup_db = 4.2 * glue;
        let gr_lin = 10.0f32.powf(-self.opto_gr_db / 20.0);
        let net_gain = gr_lin * 10.0f32.powf(makeup_db / 20.0);

        let vca_l = left * net_gain;
        let vca_r = right * net_gain;

        let max_slew = (1.6 - 0.75 * glue) * (44_100.0 / sr);
        let dl = (vca_l - self.slew_prev_l).clamp(-max_slew, max_slew);
        let dr = (vca_r - self.slew_prev_r).clamp(-max_slew, max_slew);
        let slew_l = self.slew_prev_l + dl;
        let slew_r = self.slew_prev_r + dr;
        self.slew_prev_l = slew_l;
        self.slew_prev_r = slew_r;

        let out_l = Self::peakstop_clip(slew_l);
        let out_r = Self::peakstop_clip(slew_r);

        (out_l, out_r, gr_lin.clamp(0.05, 1.0))
    }

    #[inline]
    fn peakstop_clip(x: f32) -> f32 {
        const CEIL: f32 = 0.966; // -0.3 dBFS
        const KNEE: f32 = 0.78;
        let mag = x.abs();
        if mag <= KNEE {
            x
        } else {
            let span = CEIL - KNEE;
            (KNEE + span * ((mag - KNEE) / span).tanh()).copysign(x)
        }
    }
}

// ============================================================================
// 6. Complete Ariwa Bass Instrument DSP Engine
// ============================================================================

pub struct AriwaBassDsp {
    sample_rate: f32,
    cfg: BlockConfig,
    voice: AriwaSynthVoice,
    flux_pair: [FluxWeightingPair; 2],
    head_bump: [Biquad; 2],
    dc_blocker: [Biquad; 2],
    last_drive: f32,
    dry_sub_lp: [f32; 2],
    dry_delay: [LatencyCompDelay; 2],
    filter: [Mfc42FilterChannel; 2],
    clean_tone_filter: [Mfc42FilterChannel; 2],
    oversampler: [Oversampler4x; 2],
    phaser: Mxr131AutoPhaser,
    dynamics: AriwaDynamicsGlue,
}

impl Default for AriwaBassDsp {
    fn default() -> Self {
        Self::new(44_100.0)
    }
}

impl AriwaBassDsp {
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate as f32;
        let mut dsp = Self {
            sample_rate: sr,
            cfg: BlockConfig {
                ariwa_fx_on: true,
                voice_model: BassVoiceModel::Sh101Sub,
                slope: FilterSlope::FourPole,
                sync: false,
                sync_div: SyncDivision::Quarter,
                tempo_bpm: 120.0,
                hq: true,
                gui_note: 0,
            },
            voice: AriwaSynthVoice::default(),
            flux_pair: [FluxWeightingPair::default(); 2],
            head_bump: [Biquad::default(); 2],
            dc_blocker: [Biquad::default(); 2],
            last_drive: -1.0,
            dry_sub_lp: [0.0; 2],
            dry_delay: [LatencyCompDelay::default(), LatencyCompDelay::default()],
            filter: [Mfc42FilterChannel::default(); 2],
            clean_tone_filter: [Mfc42FilterChannel::default(); 2],
            oversampler: [Oversampler4x::default(), Oversampler4x::default()],
            phaser: Mxr131AutoPhaser::default(),
            dynamics: AriwaDynamicsGlue::default(),
        };
        dsp.reset(sample_rate);
        dsp
    }

    pub fn reset(&mut self, sample_rate: f64) {
        let sr = (sample_rate as f32).max(8000.0);
        self.sample_rate = sr;
        self.voice.set_sample_rate(sr);
        self.voice.reset();
        for ch in 0..2 {
            self.flux_pair[ch].reset();
            self.head_bump[ch].reset();
            self.dc_blocker[ch].reset();
            self.dc_blocker[ch].set_highpass(5.0, 0.707, sr);
            self.dry_delay[ch].reset();
            self.filter[ch].reset();
            self.clean_tone_filter[ch].reset();
            self.oversampler[ch].reset();
        }
        self.dry_sub_lp = [0.0; 2];
        self.last_drive = -1.0;
        self.phaser.reset();
        self.dynamics.reset();
        self.update_drive_filters(0.32);
    }

    pub fn set_block(&mut self, cfg: &BlockConfig) {
        self.cfg = *cfg;
        self.voice.set_gui_note(cfg.gui_note);
    }

    pub fn latency(&self) -> u32 {
        if self.cfg.hq && self.cfg.ariwa_fx_on {
            OVERSAMPLER_LATENCY
        } else {
            0
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        self.voice.note_on(note, velocity);
    }

    pub fn note_off(&mut self, note: u8) {
        self.voice.note_off(note);
    }

    pub fn pitch_bend(&mut self, value_14bit: u16) {
        let norm = (f32::from(value_14bit) - 8192.0) / 8192.0;
        self.voice.pitch_bend_semi = norm.clamp(-1.0, 1.0) * 2.0;
    }

    pub fn control_change(&mut self, cc: u8, value: u8) {
        match cc {
            1 => {
                self.voice.mod_wheel = (f32::from(value) / 127.0).clamp(0.0, 1.0);
            }
            64 => {
                let down = value >= 64;
                self.voice.sustain_pedal = down;
                if !down {
                    self.voice.sustained_note = None;
                }
            }
            120 | 123 => {
                self.voice.all_notes_off();
            }
            _ => {}
        }
    }

    #[inline]
    fn update_drive_filters(&mut self, drive: f32) {
        if (drive - self.last_drive).abs() < 1e-3 {
            return;
        }
        self.last_drive = drive;
        let flux_boost_db = 5.5 * drive;
        let bump_db = 3.2 * drive;
        for ch in 0..2 {
            self.flux_pair[ch].update(135.0, flux_boost_db, self.sample_rate);
            self.head_bump[ch].set_peaking(61.87, 1.15, bump_db, self.sample_rate);
        }
    }

    #[inline]
    fn process_tape_stage(&mut self, x: f32, ch: usize, drive: f32) -> f32 {
        if drive < 1e-4 {
            return x;
        }
        let pre = self.flux_pair[ch].pre_emphasis(x);
        let g = 1.0 + 1.6 * drive;
        let v = pre * g;
        let sat = v
            - (0.14 * drive) * (v * v) / (1.0 + v.abs())
            - (0.08 * drive) * (v * v * v) / (1.0 + v * v);
        let de = self.flux_pair[ch].de_emphasis(sat / g);
        let bumped = self.head_bump[ch].process(de);
        self.dc_blocker[ch].process(bumped)
    }

    #[inline]
    pub fn process_frame(&mut self, c: &SampleControls) -> FrameOut {
        let sr = self.sample_rate;
        self.update_drive_filters(c.drive);

        let effective_rate_hz = if self.cfg.sync {
            let bps = (self.cfg.tempo_bpm.clamp(30.0, 300.0)) / 60.0;
            (bps * self.cfg.sync_div.cycles_per_beat()).clamp(0.05, 18.0)
        } else {
            c.speed_hz
        };

        // 1. Synthesize Oscillator + MXR 129 Sub-Octave Voice
        let (osc_l, osc_r, note_hz, opto_env, lfo_val) = self.voice.step(
            self.cfg.voice_model,
            c,
            effective_rate_hz,
            self.cfg.ariwa_fx_on,
            sr,
        );

        let active_note = if self.voice.is_gate_open() || self.voice.amp_env > 0.01 {
            self.voice.target_note
        } else {
            0
        };

        // Fast-path silence when no note is sounding: reset filter and phaser states so tails never hang
        if self.voice.amp_env < 1e-4 && !self.voice.is_gate_open() {
            for ch in 0..2 {
                self.filter[ch].reset();
                self.clean_tone_filter[ch].reset();
                self.dry_sub_lp[ch] = 0.0;
            }
            self.phaser.reset();
            return FrameOut {
                left: 0.0,
                right: 0.0,
                active_cutoff_hz: c.cutoff_hz,
                phaser_f0_hz: 380.0,
                env: 0.0,
                gr_lin: [1.0, 1.0],
                active_note: 0,
                note_hz,
            };
        }

        // 2. Otari MTR-90 2" Tape + Faraday 1/f Transformer Flux Stage
        let tape_l = self.process_tape_stage(osc_l, 0, c.drive);
        let tape_r = self.process_tape_stage(osc_r, 1, c.drive);

        // 3. Switchable "RONCA E DESLIZA" Effect vs. Clean Ariwa Dub Bass
        if !self.cfg.ariwa_fx_on {
            // Clean Dub Bass Mode: warm key-tracked low-pass tone contour + LA-4/dbx 166 Glue
            let clean_fc = (note_hz * 3.8 + 0.5 * c.cutoff_hz).clamp(90.0, 2200.0);
            let warm_l =
                self.clean_tone_filter[0].process(tape_l, clean_fc, 0.12, FilterSlope::FourPole, sr);
            let warm_r =
                self.clean_tone_filter[1].process(tape_r, clean_fc, 0.12, FilterSlope::FourPole, sr);
            let (comp_l, comp_r, gr_lin) =
                self.dynamics.process_stereo(warm_l, warm_r, c.glue, sr);
            return FrameOut {
                left: comp_l * c.out_gain,
                right: comp_r * c.out_gain,
                active_cutoff_hz: clean_fc,
                phaser_f0_hz: 380.0,
                env: opto_env,
                gr_lin: [gr_lin, gr_lin],
                active_note,
                note_hz,
            };
        }

        // 4. Key-Tracked Cutoff + Mu-Tron III Optocoupler Envelope ("Desliza") + Groove Wobble
        //    Key-track the base cutoff around C2 (65.41 Hz) so the vowel formant growl moves
        //    with the bass melody across the keyboard!
        let key_ratio = (note_hz / 65.41).powf(0.72).clamp(0.4, 3.5);
        let key_tracked_base_fc = (c.cutoff_hz * key_ratio).clamp(35.0, 2400.0);

        let env_octaves = if c.desliza < 0.0 {
            // Line 6 Filter Pro Program 41 `LOW` / Mu-Tron III `Tron Down`:
            // Starts elevated on note attack and plunges down into the sub fundamental!
            let span = (-c.desliza) * 2.2;
            span * opto_env
        } else {
            // `Tron Up` / Akai MFC42 Envelope: sweeps up through the vowel chest formants
            c.desliza * 2.2 * opto_env
        };

        let wobble_depth = (c.wobble + 0.5 * self.voice.mod_wheel).clamp(0.0, 1.0);
        // Contained LFO: +/- 0.55 octaves max (not 1.65 octaves!), smoothly gated by note decay
        let lfo_gate = (0.15 + 0.85 * self.voice.decay_env).clamp(0.0, 1.0);
        let lfo_octaves = wobble_depth * 0.55 * lfo_val * lfo_gate;
        let active_cutoff_hz = (key_tracked_base_fc * 2.0f32.powf(env_octaves + lfo_octaves))
            .clamp(30.0, (sr * 0.40).min(7500.0));

        // 5. Akai MFC42 Cascaded 2P / 4P / 8P SSM2164S State-Variable Filter
        // Dynamic resonance damping: full vocal bite and 1SS133 diode growl on note attack,
        // settling to clean warm resonance as note sustains, preventing runaway self-oscillation
        let ronca_eff = (c.ronca + 0.25 * self.voice.mod_wheel).clamp(0.0, 1.0);
        let ronca_damped = ronca_eff * (0.35 + 0.65 * self.voice.decay_env);
        let filt_l =
            self.filter[0].process(tape_l, active_cutoff_hz, ronca_damped, self.cfg.slope, sr);
        let filt_r =
            self.filter[1].process(tape_r, active_cutoff_hz, ronca_damped, self.cfg.slope, sr);

        // 6. Post-Filter NJM4558M + 1SS133 Silicon Diode Harmonic Growl (4x Oversampled in HQ mode)
        let drive = c.drive;
        let (growl_l, growl_r) = if self.cfg.hq {
            let gl = self.oversampler[0].process(filt_l, |s| mfc42_diode_growl(s, ronca_damped, drive));
            let gr = self.oversampler[1].process(filt_r, |s| mfc42_diode_growl(s, ronca_damped, drive));
            (gl, gr)
        } else {
            (
                mfc42_diode_growl(filt_l, ronca_damped, drive),
                mfc42_diode_growl(filt_r, ronca_damped, drive),
            )
        };

        // 7. MXR Model 131 Dual 6-Stage Vactrol Auto Phaser
        let (phased_l, phased_r, phaser_f0_hz) = self.phaser.process_stereo(
            growl_l,
            growl_r,
            c.phaser,
            effective_rate_hz,
            opto_env,
            sr,
        );

        // 8. SSL Console Phase-Aligned Sub-Bass Anchor (< 90 Hz)
        let delay_samples = if self.cfg.hq {
            OVERSAMPLER_LATENCY as usize
        } else {
            0
        };
        let aligned_dry_l = self.dry_delay[0].process(tape_l, delay_samples);
        let aligned_dry_r = self.dry_delay[1].process(tape_r, delay_samples);

        let sub_g = (PI * 90.0 / sr).tan();
        let sub_c = sub_g / (1.0 + sub_g);
        self.dry_sub_lp[0] += sub_c * (aligned_dry_l - self.dry_sub_lp[0]);
        self.dry_sub_lp[1] += sub_c * (aligned_dry_r - self.dry_sub_lp[1]);
        let anchor_weight = 0.55 + 0.35 * c.sub_oct;

        // 9. UREI LA-4 Optical Compressor + dbx 166 True-RMS VCA & PeakStop Ceiling Limiter
        let wet_bus_l = 0.68 * phased_l + anchor_weight * self.dry_sub_lp[0];
        let wet_bus_r = 0.68 * phased_r + anchor_weight * self.dry_sub_lp[1];
        let (comp_l, comp_r, gr_lin) =
            self.dynamics.process_stereo(wet_bus_l, wet_bus_r, c.glue, sr);

        FrameOut {
            left: comp_l * c.out_gain,
            right: comp_r * c.out_gain,
            active_cutoff_hz,
            phaser_f0_hz,
            env: opto_env,
            gr_lin: [gr_lin, gr_lin],
            active_note,
            note_hz,
        }
    }
}

/// Analytical magnitude response (in dB) of the MXR 131 6-stage Vactrol all-pass phaser comb.
pub fn phaser_response_db(freq_hz: f32, phaser_mix: f32, phaser_f0_hz: f32) -> f32 {
    let f = freq_hz.max(10.0);
    if phaser_mix <= 0.01 || f <= 65.0 {
        return 0.0;
    }
    let blend = ((f - 65.0) / 55.0).clamp(0.0, 1.0);
    let theta = -12.0 * (f / phaser_f0_hz.max(50.0)).atan();
    let depth = 0.58 * phaser_mix * blend;
    let dry_w = 1.0 - 0.38 * depth;
    let re = dry_w + depth * theta.cos();
    let im = depth * theta.sin();
    let mag_sq = (re * re + im * im).max(0.03);
    10.0 * mag_sq.log10()
}

/// Analytical magnitude response (in dB) of the cascaded Akai MFC42 2P/4P/8P SVF
/// + MXR 131 6-Stage Vactrol All-Pass Phaser notches for the Ableton Live UI display.
pub fn analytical_response_db(
    freq_hz: f32,
    cutoff_hz: f32,
    ronca: f32,
    slope: FilterSlope,
    phaser_mix: f32,
    phaser_f0_hz: f32,
) -> f32 {
    let f = freq_hz.max(10.0);
    let fc = cutoff_hz.max(25.0);
    let r = f / fc;
    let r2 = r * r;

    let total_q = 0.707 + ronca * ronca * 5.2;
    let stage_q = match slope {
        FilterSlope::TwoPole => total_q,
        FilterSlope::FourPole => total_q.powf(0.55).max(0.707),
        FilterSlope::EightPole => total_q.powf(0.35).max(0.707),
    };

    let denom_sq = (1.0 - r2) * (1.0 - r2) + (r / stage_q) * (r / stage_q);
    let stage_mag_sq = 1.0 / denom_sq.max(1e-9);
    let n_stages = slope.stages() as f32;
    let raw_filter_db = 10.0 * n_stages * stage_mag_sq.log10();
    let limited_filter_db = if raw_filter_db > 3.0 {
        3.0 + 11.0 * ((raw_filter_db - 3.0) / 11.0).tanh()
    } else {
        raw_filter_db
    };

    let phaser_db = phaser_response_db(f, phaser_mix, phaser_f0_hz);
    (limited_filter_db + phaser_db).clamp(-48.0, 18.0)
}
