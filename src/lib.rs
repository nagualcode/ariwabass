//! # Ariwa Bass — Mad Professor's 1980s/90s "Ronca e Desliza" Dub Bass Instrument
//!
//! A component-level physical & analog synthesizer bass instrument inspired by **Neil Fraser
//! (Mad Professor)** at **Ariwa Sound Studios** (London, 1980s–1990s), built with
//! [Truce](https://github.com/truce-audio/truce):
//!
//! 1. **Monophonic / Legato Dub Bass Voice + Analog RC Portamento (`Voice`, `Glide`, `Decay`)**:
//!    4 classic 1980s/1990s bass generators (`SH-101 Sub`, `Mini 3x`, `Flatwound`, `Steppers`)
//!    with 16-note last-note-priority legato stack and smooth exponential pitch portamento.
//! 2. **MXR Model 129 Companded Sub-Octave (`Sub Oct`)**:
//!    Phase-locked $-1\text{ Octave}$ ($f_0 / 2$) sub-harmonic generator shaped through Tony Gambacurta's
//!    `Am6072` $\mu$-255 differential compander and 24 dB/oct reconstruction LPF.
//! 3. **Switchable "Ronca e Desliza" Psychedelic Dub Effect (`Ariwa FX On/Off`)**:
//!    - **Key-Tracked Akai MFC42 (`SSM2164S` + `NJM2082M` 2P / 4P / 8P SVF)**:
//!      Follows the played MIDI note frequency $f_0(t)$ so the vocal formant growl (`Ronca`)
//!      stays harmonically locked to every note across the keyboard.
//!    - **Mu-Tron III (US Patent 3,911,776) & Line 6 Filter Pro (`Prog 41 LOW`) Note-Triggered
//!      CdS Optocoupler Envelope (`Desliza`) + Groove LFO (`Wobble`)**.
//!    - **4x Oversampled `1SS133` Silicon Diode / Class-A Harmonic Growl Stage**.
//!    - **Dual MXR Model 131 (M-110) 6-Stage Vactrol Auto Phaser (`Phaser`)** above $95\text{ Hz}$.
//! 4. **Otari MTR-90 2" Tape Input (`Drive`) + UREI LA-4 & dbx 166 True-RMS VCA / PeakStop (`Glue`)**.

pub mod dsp;
mod editor;
pub mod oversampling;
pub mod presets;

use std::sync::Arc;
use truce::prelude::*;

use AriwaBassParamsParamId as P;
pub use dsp::{
    AriwaBassDsp, BassVoiceModel, BlockConfig, FilterSlope, SampleControls, SyncDivision,
};

const CHUNK: usize = 64;

#[derive(Params)]
pub struct AriwaBassParams {
    /// Master switch for the signature Ariwa "Ronca e Desliza" filter/growl/phaser chain.
    /// When OFF, the instrument plays as a clean, deep Ariwa analog dub sub-bass.
    #[param(name = "Ronca & Desliza", short_name = "AriwaFX", default = true)]
    pub ariwa_fx_on: BoolParam,

    /// Bass oscillator / physical string model (`SH-101 Sub`, `Mini 3x`, `Flatwound`, `Steppers`).
    #[param(name = "Bass Voice", short_name = "Voice", default = 0)]
    pub voice_model: EnumParam<BassVoiceModel>,

    /// Base harmonic cutoff frequency of the key-tracked Akai MFC42 / Line 6 Filter Pro SVF.
    #[param(
        name = "Frequency",
        short_name = "Freq",
        range = "log(35.0, 2500.0)",
        default = 155.0,
        unit = "Hz",
        format = "format_cutoff",
        smooth = "exp(12)"
    )]
    pub cutoff: FloatParam,

    /// Coupled SSM2164 filter resonance peak and post-filter 1SS133 silicon diode throat growl.
    #[param(
        name = "Ronca",
        short_name = "Ronca",
        range = "linear(0.0, 100.0)",
        default = 8.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub ronca: FloatParam,

    /// Bipolar Mu-Tron III CdS optocoupler envelope slide (< 0% = Tron Down plunge, > 0% = Tron Up sweep).
    #[param(
        name = "Desliza",
        short_name = "Desliza",
        range = "linear(-100.0, 100.0)",
        default = 44.0,
        unit = "%",
        format = "format_signed_percent",
        smooth = "exp(12)"
    )]
    pub desliza: FloatParam,

    /// Optically smoothed Groove LFO modulation depth.
    #[param(
        name = "Wobble",
        short_name = "Wobble",
        range = "linear(0.0, 100.0)",
        default = 0.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub wobble: FloatParam,

    /// LFO sweep speed and optocoupler release glide speed.
    #[param(
        name = "Speed",
        short_name = "Speed",
        range = "log(0.08, 12.0)",
        default = 1.25,
        unit = "Hz",
        format = "format_hz",
        smooth = "exp(15)"
    )]
    pub speed: FloatParam,

    /// Sync LFO wobble to host tempo.
    #[param(name = "Sync", short_name = "Sync", default = false)]
    pub sync: BoolParam,

    /// Musical note division when Tempo Sync is enabled.
    #[param(name = "Division", short_name = "Div", default = 2)]
    pub sync_div: EnumParam<SyncDivision>,

    /// Akai MFC42 cascaded SVF slope: 2-Pole (-12 dB/oct), 4-Pole (-24 dB/oct), or 8-Pole (-48 dB/oct).
    #[param(name = "Slope", short_name = "Slope", default = 1)]
    pub slope: EnumParam<FilterSlope>,

    /// Analog RC pitch portamento / legato note slide time in milliseconds.
    #[param(
        name = "Glide",
        short_name = "Glide",
        range = "linear(0.0, 500.0)",
        default = 45.0,
        unit = "ms",
        format = "format_ms",
        smooth = "exp(12)"
    )]
    pub glide: FloatParam,

    /// MXR Model 129 Pitch Transposer -1 Octave companded sub-bass level.
    #[param(
        name = "Sub Oct",
        short_name = "SubOct",
        range = "linear(0.0, 100.0)",
        default = 36.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub sub_oct: FloatParam,

    /// Pluck & sustain contour decay time in milliseconds.
    #[param(
        name = "Decay",
        short_name = "Decay",
        range = "log(80.0, 2500.0)",
        default = 450.0,
        unit = "ms",
        format = "format_ms",
        smooth = "exp(12)"
    )]
    pub decay: FloatParam,

    /// Otari MTR-90 2" tape head bump (62 Hz), Faraday 1/f transformer flux, and MFC42 diode saturation.
    #[param(
        name = "Drive",
        short_name = "Drive",
        range = "linear(0.0, 100.0)",
        default = 28.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub drive: FloatParam,

    /// Dual MXR Model 131 (M-110) 6-Stage Vactrol Auto Phaser depth above 95 Hz.
    #[param(
        name = "Phaser",
        short_name = "Phaser",
        range = "linear(0.0, 100.0)",
        default = 16.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub phaser: FloatParam,

    /// UREI LA-4 optical leveler + dbx 166 Blackmer True-RMS VCA compression & PeakStop limiter.
    #[param(
        name = "Glue",
        short_name = "Glue",
        range = "linear(0.0, 100.0)",
        default = 55.0,
        unit = "%",
        format = "format_percent",
        smooth = "exp(12)"
    )]
    pub glue: FloatParam,

    /// Final output gain trim in dB.
    #[param(
        name = "Out Gain",
        short_name = "Out",
        range = "linear(-24.0, 24.0)",
        default = 0.0,
        unit = "dB",
        smooth = "exp(12)"
    )]
    pub out_gain: FloatParam,

    /// Run the Akai MFC42 1SS133 silicon diode growl stage at 4x linear-phase oversampling.
    #[param(name = "HQ 4x", short_name = "HQ", default = true)]
    pub hq: BoolParam,

    /// Audition MIDI note triggered from the on-screen Ableton-style bass keyboard (0 = none).
    #[param(
        name = "GUI Note",
        short_name = "Key",
        range = "discrete(0, 127)",
        default = 0
    )]
    pub gui_note: IntParam,

    // --- Real-Time UI Telemetry Meters ---
    #[meter]
    pub meter_left: MeterSlot,
    #[meter]
    pub meter_right: MeterSlot,
    #[meter]
    pub meter_gr_left: MeterSlot,
    #[meter]
    pub meter_gr_right: MeterSlot,
    #[meter]
    pub meter_cutoff_hz: MeterSlot,
    #[meter]
    pub meter_phaser_hz: MeterSlot,
    #[meter]
    pub meter_env: MeterSlot,
    #[meter]
    pub meter_active_note: MeterSlot,
    #[meter]
    pub meter_note_hz: MeterSlot,
}

impl AriwaBassParams {
    fn format_cutoff(&self, value: f64) -> String {
        if value >= 1000.0 {
            format!("{:.2} kHz", value * 0.001)
        } else {
            format!("{value:.0} Hz")
        }
    }

    fn format_hz(&self, value: f64) -> String {
        format!("{value:.2} Hz")
    }

    fn format_ms(&self, value: f64) -> String {
        if value >= 1000.0 {
            format!("{:.2} s", value * 0.001)
        } else {
            format!("{value:.0} ms")
        }
    }

    fn format_percent(&self, value: f64) -> String {
        format!("{value:.0} %")
    }

    fn format_signed_percent(&self, value: f64) -> String {
        if value > 0.5 {
            format!("+{value:.0} %")
        } else if value < -0.5 {
            format!("{value:.0} %")
        } else {
            "0 %".to_owned()
        }
    }

    fn block_config(&self, tempo_bpm: f32) -> BlockConfig {
        BlockConfig {
            ariwa_fx_on: self.ariwa_fx_on.value(),
            voice_model: self.voice_model.value(),
            slope: self.slope.value(),
            sync: self.sync.value(),
            sync_div: self.sync_div.value(),
            tempo_bpm,
            hq: self.hq.value(),
            gui_note: self.gui_note.value().clamp(0, 127) as u8,
        }
    }
}

#[inline]
fn dispatch_midi_event(state: &mut AriwaBassDsp, body: &EventBody) {
    match *body {
        EventBody::NoteOn { note, velocity, .. } => {
            if velocity == 0 {
                state.note_off(note);
            } else {
                state.note_on(note, velocity);
            }
        }
        EventBody::NoteOff { note, .. } => {
            state.note_off(note);
        }
        EventBody::NoteOn2 { note, velocity, .. } => {
            let vel7 = ((velocity >> 9) as u8).max(u8::from(velocity > 0));
            if vel7 == 0 {
                state.note_off(note);
            } else {
                state.note_on(note, vel7);
            }
        }
        EventBody::NoteOff2 { note, .. } => {
            state.note_off(note);
        }
        EventBody::PitchBend { value, .. } => {
            state.pitch_bend(value);
        }
        EventBody::ControlChange { cc, value, .. } => {
            state.control_change(cc, value);
        }
        _ => {}
    }
}

pub struct AriwaBass;

impl PluginLogic for AriwaBass {
    type Params = AriwaBassParams;
    type DspState = AriwaBassDsp;

    fn bus_layouts() -> Vec<BusLayout> {
        BusLayout::stereo_and_mono_output()
    }

    fn init(params: &AriwaBassParams, _cx: &InitContext) -> AriwaBassDsp {
        let mut dsp = AriwaBassDsp::new(44_100.0);
        dsp.set_block(&params.block_config(120.0));
        dsp
    }

    fn reset(state: &mut AriwaBassDsp, params: &AriwaBassParams, config: &AudioConfig) {
        state.reset(config.sample_rate);
        state.set_block(&params.block_config(120.0));
    }

    fn latency(state: &AriwaBassDsp) -> u32 {
        state.latency()
    }

    fn process(
        state: &mut AriwaBassDsp,
        params: &AriwaBassParams,
        buffer: &mut AudioBuffer,
        events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let num_samples = buffer.num_samples();
        if num_samples == 0 {
            for ev in events.iter() {
                dispatch_midi_event(state, &ev.body);
            }
            return ProcessStatus::Normal;
        }

        let bpm = if context.transport.tempo > 20.0 {
            context.transport.tempo as f32
        } else {
            120.0
        };
        let cfg = params.block_config(bpm);
        state.set_block(&cfg);

        let mut cutoff = [0.0f32; CHUNK];
        let mut ronca = [0.0f32; CHUNK];
        let mut desliza = [0.0f32; CHUNK];
        let mut wobble = [0.0f32; CHUNK];
        let mut speed = [0.0f32; CHUNK];
        let mut glide = [0.0f32; CHUNK];
        let mut sub_oct = [0.0f32; CHUNK];
        let mut decay = [0.0f32; CHUNK];
        let mut drive = [0.0f32; CHUNK];
        let mut phaser = [0.0f32; CHUNK];
        let mut glue = [0.0f32; CHUNK];
        let mut out_db = [0.0f32; CHUNK];

        let mut min_gr = [1.0f32; 2];
        let mut max_env = 0.0f32;
        let mut last_fc = params.cutoff.value();
        let mut last_ph_fc = 380.0f32;
        let mut last_active_note = 0u8;
        let mut last_note_hz = 65.41f32;
        let mut cached_db = f32::NAN;
        let mut cached_lin = 1.0f32;

        let mut ev_iter = events.iter().peekable();

        let mut start = 0;
        while start < num_samples {
            let len = CHUNK.min(num_samples - start);
            params.cutoff.read_into(&mut cutoff[..len]);
            params.ronca.read_into(&mut ronca[..len]);
            params.desliza.read_into(&mut desliza[..len]);
            params.wobble.read_into(&mut wobble[..len]);
            params.speed.read_into(&mut speed[..len]);
            params.glide.read_into(&mut glide[..len]);
            params.sub_oct.read_into(&mut sub_oct[..len]);
            params.decay.read_into(&mut decay[..len]);
            params.drive.read_into(&mut drive[..len]);
            params.phaser.read_into(&mut phaser[..len]);
            params.glue.read_into(&mut glue[..len]);
            params.out_gain.read_into(&mut out_db[..len]);

            let mut i = 0;
            buffer.slice(start, len).for_each_stereo_frame(|_inp, out| {
                let global_sample = start + i;
                while let Some(ev) = ev_iter.peek() {
                    if (ev.sample_offset as usize) <= global_sample {
                        dispatch_midi_event(state, &ev.body);
                        ev_iter.next();
                    } else {
                        break;
                    }
                }

                if out_db[i] != cached_db {
                    cached_db = out_db[i];
                    cached_lin = db_to_linear(cached_db);
                }
                let controls = SampleControls {
                    cutoff_hz: cutoff[i],
                    ronca: ronca[i] * 0.01,
                    desliza: desliza[i] * 0.01,
                    wobble: wobble[i] * 0.01,
                    speed_hz: speed[i],
                    glide_ms: glide[i],
                    sub_oct: sub_oct[i] * 0.01,
                    decay_ms: decay[i],
                    drive: drive[i] * 0.01,
                    phaser: phaser[i] * 0.01,
                    glue: glue[i] * 0.01,
                    out_gain: cached_lin,
                };
                let f = state.process_frame(&controls);
                out[0] = f.left;
                out[1] = f.right;
                min_gr[0] = min_gr[0].min(f.gr_lin[0]);
                min_gr[1] = min_gr[1].min(f.gr_lin[1]);
                max_env = max_env.max(f.env);
                last_fc = f.active_cutoff_hz;
                last_ph_fc = f.phaser_f0_hz;
                last_active_note = f.active_note;
                last_note_hz = f.note_hz;
                i += 1;
            });
            start += len;
        }

        // Flush any trailing MIDI events scheduled at or past buffer end
        for ev in ev_iter {
            dispatch_midi_event(state, &ev.body);
        }

        let outs = buffer.num_output_channels();
        if outs >= 1 {
            context.set_meter(P::MeterLeft, buffer.output_peak(0));
        }
        context.set_meter(
            P::MeterRight,
            buffer.output_peak(if outs >= 2 { 1 } else { 0 }),
        );
        let gr_db_l = (-20.0 * min_gr[0].max(0.05).log10()).max(0.0);
        let gr_db_r = (-20.0 * min_gr[1].max(0.05).log10()).max(0.0);
        context.set_meter(P::MeterGrLeft, gr_db_l);
        context.set_meter(P::MeterGrRight, gr_db_r);
        context.set_meter(P::MeterCutoffHz, last_fc);
        context.set_meter(P::MeterPhaserHz, last_ph_fc);
        context.set_meter(P::MeterEnv, max_env);
        context.set_meter(P::MeterActiveNote, f32::from(last_active_note));
        context.set_meter(P::MeterNoteHz, last_note_hz);

        ProcessStatus::Normal
    }

    fn editor(params: Arc<AriwaBassParams>) -> Box<dyn Editor> {
        editor::create(params)
    }
}

truce::plugin! {
    logic: AriwaBass,
    params: AriwaBassParams,
}

truce::enable_rt_paranoid!();

#[cfg(test)]
mod tests {
    use super::*;

    fn default_controls() -> SampleControls {
        SampleControls {
            cutoff_hz: 155.0,
            ronca: 0.08,
            desliza: 0.44,
            wobble: 0.0,
            speed_hz: 1.25,
            glide_ms: 45.0,
            sub_oct: 0.36,
            decay_ms: 450.0,
            drive: 0.28,
            phaser: 0.16,
            glue: 0.55,
            out_gain: 1.0,
        }
    }

    #[test]
    fn info_is_valid() {
        truce_test::assert_valid_info::<Plugin>();
    }

    #[test]
    fn has_editor() {
        truce_test::assert_has_editor::<Plugin>();
    }

    #[test]
    fn state_round_trips() {
        truce_test::assert_state_round_trip::<Plugin>();
    }

    #[test]
    fn process_is_realtime_clean() {
        use std::time::Duration;
        use truce_test::{assert_realtime_clean, driver};
        assert_realtime_clean(|| {
            driver!(Plugin)
                .duration(Duration::from_millis(90))
                .script(|s| {
                    s.note_on(36, 0.85);
                    s.wait_ms(20);
                    s.set_param(P::Cutoff, 180.0);
                    s.note_on(43, 0.90); // legato slide to G1
                    s.wait_ms(20);
                    s.set_param(P::Ronca, 75.0);
                    s.set_param(P::Desliza, -60.0);
                    s.wait_ms(20);
                    s.note_off(43);
                    s.note_off(36);
                    s.set_param(P::AriwaFxOn, 0.0);
                    s.note_on(38, 0.80);
                    s.wait_ms(20);
                })
                .run()
        });
    }

    #[test]
    fn instrument_stays_silent_until_midi_note_and_tracks_played_pitch() {
        let sr = 48_000.0f32;
        let mut dsp = AriwaBassDsp::new(f64::from(sr));
        let mut c = default_controls();
        c.glide_ms = 0.0;
        c.wobble = 0.0;
        c.desliza = 0.0;
        c.sub_oct = 0.0;

        // 1. Without a MIDI note, instrument must remain completely silent (never droning!)
        for _ in 0..512 {
            let f = dsp.process_frame(&c);
            assert_eq!(f.left, 0.0);
            assert_eq!(f.right, 0.0);
            assert_eq!(f.active_note, 0);
        }

        // 2. Trigger A1 (MIDI 33 = 55 Hz) and verify pitch & audio generation
        dsp.note_on(33, 110);
        let mut peak_a1 = 0.0f32;
        let mut hz_a1 = 0.0f32;
        for _ in 0..2400 {
            let f = dsp.process_frame(&c);
            peak_a1 = peak_a1.max(f.left.abs());
            hz_a1 = f.note_hz;
        }
        assert!(peak_a1 > 0.15, "Instrument must generate strong bass output on NoteOn (got {peak_a1})");
        assert!(
            (hz_a1 - 55.0).abs() < 0.5,
            "MIDI note 33 must synthesize 55.0 Hz fundamental (got {hz_a1} Hz)"
        );

        // 3. Trigger A2 (MIDI 45 = 110 Hz) and verify both note_hz and key-tracked filter follow the note
        let fc_a1 = dsp.process_frame(&c).active_cutoff_hz;
        dsp.note_on(45, 110);
        let mut hz_a2 = 0.0f32;
        let mut fc_a2 = 0.0f32;
        for _ in 0..1200 {
            let f = dsp.process_frame(&c);
            hz_a2 = f.note_hz;
            fc_a2 = f.active_cutoff_hz;
        }
        assert!(
            (hz_a2 - 110.0).abs() < 0.5,
            "MIDI note 45 must synthesize 110.0 Hz fundamental (got {hz_a2} Hz)"
        );
        assert!(
            fc_a2 > fc_a1 * 1.4,
            "Key-tracked filter cutoff must track upward from A1 ({fc_a1} Hz) to A2 ({fc_a2} Hz)"
        );
    }

    #[test]
    fn all_four_bass_voice_models_produce_distinct_articulate_audio() {
        let sr = 48_000.0;
        let c = default_controls();
        for model in [
            BassVoiceModel::Sh101Sub,
            BassVoiceModel::Minimoog3x,
            BassVoiceModel::FlatwoundJazz,
            BassVoiceModel::SteppersReese,
        ] {
            let mut dsp = AriwaBassDsp::new(sr);
            dsp.set_block(&BlockConfig {
                ariwa_fx_on: true,
                voice_model: model,
                slope: FilterSlope::FourPole,
                sync: false,
                sync_div: SyncDivision::Quarter,
                tempo_bpm: 120.0,
                hq: true,
                gui_note: 0,
            });
            dsp.note_on(36, 115); // C2
            let mut energy = 0.0f32;
            for _ in 0..4800 {
                let f = dsp.process_frame(&c);
                energy += f.left * f.left + f.right * f.right;
            }
            let model_idx = model as usize;
            assert!(
                energy > 10.0,
                "Voice model {model_idx} must produce strong bass energy (got {energy})"
            );
        }
    }

    #[test]
    fn mfc42_filter_slopes_monotonically_steepen_from_2p_to_8p() {
        let fc = 150.0;
        let f_stop = 600.0;
        let db_2p = dsp::analytical_response_db(f_stop, fc, 0.2, FilterSlope::TwoPole, 0.0, 380.0);
        let db_4p = dsp::analytical_response_db(f_stop, fc, 0.2, FilterSlope::FourPole, 0.0, 380.0);
        let db_8p = dsp::analytical_response_db(f_stop, fc, 0.2, FilterSlope::EightPole, 0.0, 380.0);
        assert!(
            db_2p > db_4p && db_4p > db_8p,
            "Expected 2P ({db_2p:.1} dB) > 4P ({db_4p:.1} dB) > 8P ({db_8p:.1} dB) at 2 octaves above fc"
        );
        assert!(db_8p <= -40.0, "8P must provide brickwall attenuation ({db_8p:.1} dB)");
    }

    #[test]
    fn class_a_and_mfc42_diode_growl_generate_even_and_odd_harmonics() {
        let sr = 48_000.0f32;
        let f0 = 100.0f32;
        let n_samples = 48_000usize;
        let mut h1_re = 0.0f64;
        let mut h1_im = 0.0f64;
        let mut h2_re = 0.0f64;
        let mut h2_im = 0.0f64;
        let mut h3_re = 0.0f64;
        let mut h3_im = 0.0f64;

        for n in 0..n_samples {
            let t = n as f32 / sr;
            let x = 0.6 * (std::f32::consts::TAU * f0 * t).sin();
            let y = dsp::mfc42_diode_growl(x, 0.75, 0.65) as f64;
            let phi = std::f64::consts::TAU * f64::from(f0) * (n as f64 / f64::from(sr));
            h1_re += y * phi.cos();
            h1_im += y * phi.sin();
            h2_re += y * (2.0 * phi).cos();
            h2_im += y * (2.0 * phi).sin();
            h3_re += y * (3.0 * phi).cos();
            h3_im += y * (3.0 * phi).sin();
        }
        let h1 = (h1_re.hypot(h1_im) * 2.0 / n_samples as f64) as f32;
        let h2 = (h2_re.hypot(h2_im) * 2.0 / n_samples as f64) as f32;
        let h3 = (h3_re.hypot(h3_im) * 2.0 / n_samples as f64) as f32;

        assert!(h1 > 0.3, "Fundamental H1 must remain strong ({h1})");
        assert!(h2 > 0.01, "Must generate genuine H2 warmth ({h2})");
        assert!(h3 > 0.002, "Must generate genuine H3 diode growl ({h3})");
        assert!(
            h2 > h3,
            "Class-A / asymmetric diode stage must produce 2nd harmonic H2 ({h2}) dominance over H3 ({h3})"
        );
    }

    #[test]
    fn desliza_envelope_sweeps_up_and_tron_down_on_note_attack() {
        let sr = 48_000.0;
        let mut dsp_up = AriwaBassDsp::new(sr);
        let mut dsp_down = AriwaBassDsp::new(sr);
        let mut up_c = default_controls();
        up_c.cutoff_hz = 200.0;
        up_c.desliza = 0.80;
        up_c.wobble = 0.0;

        let mut down_c = up_c;
        down_c.desliza = -0.80;

        dsp_up.note_on(36, 120);
        dsp_down.note_on(36, 120);

        let mut peak_fc_up = 0.0f32;
        let mut end_fc_down = 0.0f32;
        let mut peak_fc_down = 0.0f32;
        for _ in 0..18_000 {
            let fu = dsp_up.process_frame(&up_c);
            let fd = dsp_down.process_frame(&down_c);
            peak_fc_up = peak_fc_up.max(fu.active_cutoff_hz);
            peak_fc_down = peak_fc_down.max(fd.active_cutoff_hz);
            end_fc_down = fd.active_cutoff_hz;
        }
        assert!(
            peak_fc_up > 450.0,
            "Positive Desliza (Tron Up) must sweep cutoff upward on note attack (got {peak_fc_up} Hz)"
        );
        assert!(
            peak_fc_down > end_fc_down * 1.4,
            "Negative Desliza (Program 41 Tron Down) must start high ({peak_fc_down} Hz) and plunge down into the sub ({end_fc_down} Hz)"
        );
    }

    #[test]
    fn mxr129_sub_octave_generates_half_frequency_subharmonic() {
        let sr = 48_000.0f32;
        let mut dsp = AriwaBassDsp::new(f64::from(sr));
        let mut c = default_controls();
        c.cutoff_hz = 400.0;
        c.ronca = 0.2;
        c.desliza = 0.0;
        c.wobble = 0.0;
        c.glide_ms = 0.0;
        c.sub_oct = 0.90;

        // Play A2 (MIDI 45 = 110 Hz); sub-octave is A1 (55 Hz)
        dsp.note_on(45, 115);
        let f_sub = 55.0f32;
        let mut sub_re = 0.0f64;
        let mut sub_im = 0.0f64;
        let n_total = 24_000usize;
        for n in 0..n_total {
            let y = dsp.process_frame(&c).left as f64;
            if n >= 4000 {
                let phi = std::f64::consts::TAU * f64::from(f_sub) * (n as f64 / f64::from(sr));
                sub_re += y * phi.cos();
                sub_im += y * phi.sin();
            }
        }
        let sub_mag = (sub_re.hypot(sub_im) * 2.0 / (n_total - 4000) as f64) as f32;
        assert!(
            sub_mag > 0.05,
            "MXR 129 sub-octave engine must generate strong 55 Hz sub-harmonic on A2 note (got {sub_mag})"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn gui_screenshot_macos() {
        truce_test::screenshot!(Plugin, "screenshots/ariwa_bass_macos.png").run();
    }
}
