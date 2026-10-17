//! Modern, flat-dark interface inspired by Ableton Live 12 instruments (Analog / Drift / Meld / Auto Filter),
//! tailored for Mad Professor's 1980s/1990s Ariwa Studios "Ronca e Desliza" Dub Bass Instrument.

use std::collections::VecDeque;
use std::sync::Arc;

use egui::{
    Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, pos2, vec2,
};
use truce::prelude::*;
use truce_core::editor::{PluginContext, PluginContextReadF32};
use truce_egui::EguiEditor;
use truce_params::Params;

use crate::AriwaBassParams;
use crate::AriwaBassParamsParamId as P;
use crate::dsp::{FilterSlope, analytical_response_db};
use crate::presets::FACTORY_PRESETS;

pub const WIDTH: u32 = 760;
pub const HEIGHT: u32 = 342;
const HEADER_H: f32 = 28.0;
const KEYBOARD_H: f32 = 36.0;

// --- Ableton Live Dark Palette + Ariwa Rack Hardware Accents ---
const BG: Color32 = Color32::from_rgb(38, 38, 38);
const PANEL: Color32 = Color32::from_rgb(46, 46, 46);
const HEADER: Color32 = Color32::from_rgb(52, 52, 52);
const DISPLAY: Color32 = Color32::from_rgb(20, 20, 22);
const GRID: Color32 = Color32::from_rgb(42, 42, 46);
const TEXT: Color32 = Color32::from_rgb(218, 218, 220);
const TEXT_DIM: Color32 = Color32::from_rgb(138, 138, 142);
const TRACK: Color32 = Color32::from_rgb(76, 76, 80);

// Hardware section accent colors:
/// Akai MFC42 / Ableton Auto Filter warm amber
const ACCENT_FILTER: Color32 = Color32::from_rgb(255, 160, 40);
/// Line 6 Filter Pro purple/violet ("Ronca" resonance & diode growl)
const ACCENT_RONCA: Color32 = Color32::from_rgb(186, 124, 255);
/// Mu-Tron III / Groove Modulator electric cyan ("Desliza" & Wobble)
const ACCENT_SLIDE: Color32 = Color32::from_rgb(90, 200, 250);
/// MXR Series 129 / 131 anodized blue (Sub-Octave & 6-Stage Vactrol Phaser)
const ACCENT_MXR: Color32 = Color32::from_rgb(58, 156, 255);
/// Otari 2" Tape / MFC42 1SS133 Diode Drive warm coral
const ACCENT_DRIVE: Color32 = Color32::from_rgb(255, 114, 76);
/// UREI LA-4 / dbx 166 dynamics green
const ACCENT_GLUE: Color32 = Color32::from_rgb(72, 204, 96);

const METER_LOW: Color32 = Color32::from_rgb(70, 200, 90);
const METER_MID: Color32 = Color32::from_rgb(235, 210, 60);
const METER_HIGH: Color32 = Color32::from_rgb(235, 70, 60);

const DB_MIN: f32 = -60.0;
const GR_RANGE: f32 = 18.0;
const HISTORY_LEN: usize = 220;

const FREQ_MIN: f32 = 25.0;
const FREQ_MAX: f32 = 12_000.0;
const PLOT_DB_MIN: f32 = -36.0;
const PLOT_DB_MAX: f32 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum DisplayView {
    FilterPhaser,
    GlideActivity,
}

struct UiState {
    view: DisplayView,
    current_preset: usize,
    out_peak_db: [f32; 2],
    gr_db: [f32; 2],
    active_cutoff_hz: f32,
    phaser_f0_hz: f32,
    env_val: f32,
    active_note: u8,
    note_hz: f32,
    gui_pressed_note: u8,
    /// `(env_0_to_1, cutoff_norm_0_to_1, gr_db)`
    history: VecDeque<(f32, f32, f32)>,
}

pub(crate) fn create(params: Arc<AriwaBassParams>) -> Box<dyn Editor> {
    let mut state = UiState {
        view: DisplayView::FilterPhaser,
        current_preset: 0,
        out_peak_db: [DB_MIN; 2],
        gr_db: [0.0; 2],
        active_cutoff_hz: 165.0,
        phaser_f0_hz: 380.0,
        env_val: 0.0,
        active_note: 0,
        note_hz: 65.41,
        gui_pressed_note: 0,
        history: VecDeque::with_capacity(HISTORY_LEN),
    };
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    EguiEditor::new(
        params,
        (WIDTH, HEIGHT),
        move |ui: &mut egui::Ui, cx: &PluginContext<AriwaBassParams>| {
            state.draw(ui, cx);
        },
    )
    .with_visuals(visuals)
    .into_editor()
}

#[inline]
fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-9 {
        20.0 * lin.log10()
    } else {
        -180.0
    }
}

#[inline]
fn freq_to_norm(f: f32) -> f32 {
    ((f.clamp(FREQ_MIN, FREQ_MAX) / FREQ_MIN).ln() / (FREQ_MAX / FREQ_MIN).ln()).clamp(0.0, 1.0)
}

#[inline]
fn norm_to_freq(n: f32) -> f32 {
    FREQ_MIN * (FREQ_MAX / FREQ_MIN).powf(n.clamp(0.0, 1.0))
}

#[inline]
fn midi_note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(note % 12) as usize];
    let octave = (note as i32 / 12) - 1;
    format!("{name}{octave}")
}

impl UiState {
    fn draw(&mut self, ui: &mut egui::Ui, cx: &PluginContext<AriwaBassParams>) {
        ui.ctx().request_repaint();
        let dt = ui.input(|i| i.stable_dt).clamp(0.001, 0.1);
        self.update_telemetry(cx, dt);

        let full = ui.max_rect();
        let painter = ui.painter_at(full);
        painter.rect_filled(full, 0.0, BG);

        // 1. Header Bar
        let header = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
        self.draw_header(ui, &painter, header, cx);

        // 2. Bottom Interactive 2-Octave Dub Bass Keyboard (C1..C3)
        let kbd_rect = Rect::from_min_max(
            pos2(full.left() + 10.0, full.bottom() - KEYBOARD_H - 6.0),
            pos2(full.right() - 10.0, full.bottom() - 6.0),
        );
        self.draw_keyboard(ui, &painter, kbd_rect, cx);

        let body = Rect::from_min_max(
            pos2(full.left() + 10.0, header.bottom() + 6.0),
            pos2(full.right() - 10.0, kbd_rect.top() - 6.0),
        );

        // 3. Left Column: Interactive Display + View Tabs + Slope Pills
        let display_w = 276.0;
        let top_bar = Rect::from_min_size(body.min, vec2(display_w, 18.0));
        self.draw_display_topbar(ui, top_bar, cx);

        let display_rect = Rect::from_min_max(
            pos2(body.left(), top_bar.bottom() + 4.0),
            pos2(body.left() + display_w, body.bottom()),
        );
        painter.rect_filled(display_rect, 3.0, DISPLAY);
        painter.rect_stroke(
            display_rect,
            3.0,
            Stroke::new(1.0_f32, GRID),
            StrokeKind::Inside,
        );
        match self.view {
            DisplayView::FilterPhaser => self.draw_filter_phaser_view(ui, &painter, display_rect, cx),
            DisplayView::GlideActivity => self.draw_activity_view(&painter, display_rect),
        }

        // 4. Right Column: Stereo GR + Output Meters
        let meters_rect = Rect::from_min_max(pos2(body.right() - 74.0, body.top()), body.max);
        self.draw_meters(&painter, meters_rect);

        // 5. Center Column: Bass Voice Selector + Ableton Live Control Strips
        let centre = Rect::from_min_max(
            pos2(display_rect.right() + 12.0, body.top()),
            pos2(meters_rect.left() - 12.0, body.bottom()),
        );
        self.draw_controls(ui, &painter, centre, cx);
    }

    fn update_telemetry(&mut self, cx: &PluginContext<AriwaBassParams>, dt: f32) {
        let fall = 22.0 * dt;
        for (ch, id) in [P::MeterLeft, P::MeterRight].into_iter().enumerate() {
            let v = lin_to_db(cx.get_meter(id)).max(DB_MIN);
            self.out_peak_db[ch] = if v > self.out_peak_db[ch] - fall {
                v
            } else {
                self.out_peak_db[ch] - fall
            };
        }

        for (ch, id) in [P::MeterGrLeft, P::MeterGrRight].into_iter().enumerate() {
            let gr = cx.get_meter(id).clamp(0.0, GR_RANGE);
            self.gr_db[ch] = if gr > self.gr_db[ch] {
                gr
            } else {
                (self.gr_db[ch] - 35.0 * dt).max(gr)
            };
        }

        let raw_fc = cx.get_meter(P::MeterCutoffHz);
        let base_fc = cx.get_param_plain(P::Cutoff);
        let target_fc = if raw_fc > 20.0 { raw_fc } else { base_fc };
        self.active_cutoff_hz += (target_fc - self.active_cutoff_hz) * (18.0 * dt).min(1.0);

        let raw_ph = cx.get_meter(P::MeterPhaserHz);
        if raw_ph > 40.0 {
            self.phaser_f0_hz += (raw_ph - self.phaser_f0_hz) * (15.0 * dt).min(1.0);
        }

        self.env_val = cx.get_meter(P::MeterEnv).clamp(0.0, 1.0);
        self.active_note = cx.get_meter(P::MeterActiveNote).round().clamp(0.0, 127.0) as u8;
        let hz = cx.get_meter(P::MeterNoteHz);
        if hz > 15.0 {
            self.note_hz = hz;
        }

        if self.history.len() == HISTORY_LEN {
            self.history.pop_front();
        }
        let fc_norm = freq_to_norm(self.active_cutoff_hz);
        self.history.push_back((
            self.env_val,
            fc_norm,
            self.gr_db[0].max(self.gr_db[1]),
        ));
    }

    fn draw_header(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        header: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        painter.rect_filled(header, 0.0, HEADER);

        // Master "Ronca e Desliza" FX On/Off Toggle (switches between Psychedelic Growl/Slide vs Clean Dub Sub)
        let fx_id = u32::from(P::AriwaFxOn);
        let fx_on = cx.get_param(fx_id) > 0.5;
        let on_btn = Rect::from_min_size(
            pos2(header.left() + 10.0, header.top() + 5.0),
            vec2(104.0, 18.0),
        );
        let resp = ui
            .interact(on_btn, ui.id().with("ariwa_fx_toggle"), Sense::click())
            .on_hover_text(
                "Toggle the signature Ariwa 'Ronca e Desliza' filter/growl/phaser chain On/Off (Clean Dub Sub when Off)",
            );
        if resp.clicked() {
            cx.begin_edit(fx_id);
            cx.set_param(fx_id, if fx_on { 0.0 } else { 1.0 });
            cx.end_edit(fx_id);
        }
        let fx_on = cx.get_param(fx_id) > 0.5;
        painter.rect_filled(
            on_btn,
            2.0,
            if fx_on { ACCENT_FILTER } else { PANEL },
        );
        painter.circle_filled(
            pos2(on_btn.left() + 9.0, on_btn.center().y),
            3.5,
            if fx_on { Color32::BLACK } else { ACCENT_GLUE },
        );
        painter.text(
            pos2(on_btn.left() + 18.0, on_btn.center().y),
            Align2::LEFT_CENTER,
            if fx_on { "RONCA FX: ON" } else { "CLEAN SUB" },
            FontId::proportional(9.5),
            if fx_on { Color32::BLACK } else { TEXT },
        );

        // Plugin Title & Instrument Lineage Subtitle
        painter.text(
            pos2(on_btn.right() + 10.0, header.center().y),
            Align2::LEFT_CENTER,
            "ARIWA BASS",
            FontId::proportional(12.5),
            TEXT,
        );
        painter.text(
            pos2(on_btn.right() + 92.0, header.center().y),
            Align2::LEFT_CENTER,
            "Dub Bass Instrument • Mad Professor '80s/'90s",
            FontId::proportional(10.0),
            TEXT_DIM,
        );

        // Preset Selector Bar
        let preset_rect = Rect::from_min_size(
            pos2(header.right() - 278.0, header.top() + 5.0),
            vec2(216.0, 18.0),
        );
        self.draw_preset_bar(ui, painter, preset_rect, cx);

        // HQ 4x Oversampling Button
        let hq_rect = Rect::from_min_size(
            pos2(header.right() - 54.0, header.top() + 5.0),
            vec2(44.0, 18.0),
        );
        toggle_btn(
            ui,
            cx,
            hq_rect,
            P::Hq,
            "HQ 4x",
            ACCENT_FILTER,
            "4x linear-phase Kaiser oversampling on MFC42 1SS133 diode growl",
        );
    }

    fn draw_preset_bar(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        let prev_r = Rect::from_min_size(rect.min, vec2(18.0, rect.height()));
        let next_r = Rect::from_min_size(
            pos2(rect.right() - 18.0, rect.top()),
            vec2(18.0, rect.height()),
        );
        let mid_r = Rect::from_min_max(
            pos2(prev_r.right() + 2.0, rect.top()),
            pos2(next_r.left() - 2.0, rect.bottom()),
        );

        for (r, label, delta) in [(prev_r, "<", -1isize), (next_r, ">", 1isize)] {
            let resp = ui.interact(r, ui.id().with(("preset_nav", delta)), Sense::click());
            painter.rect_filled(r, 1.5, if resp.hovered() { TRACK } else { DISPLAY });
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(10.0),
                TEXT,
            );
            if resp.clicked() {
                let len = FACTORY_PRESETS.len() as isize;
                self.current_preset =
                    ((self.current_preset as isize + delta).rem_euclid(len)) as usize;
                self.apply_preset(cx, self.current_preset);
            }
        }

        let preset = &FACTORY_PRESETS[self.current_preset];
        let mid_resp = ui
            .interact(mid_r, ui.id().with("preset_mid"), Sense::click())
            .on_hover_text(preset.subtitle);
        if mid_resp.clicked() {
            self.current_preset = (self.current_preset + 1) % FACTORY_PRESETS.len();
            self.apply_preset(cx, self.current_preset);
        }
        painter.rect_filled(mid_r, 1.5, DISPLAY);
        painter.text(
            mid_r.center(),
            Align2::CENTER_CENTER,
            preset.name,
            FontId::proportional(9.5),
            ACCENT_FILTER,
        );
    }

    fn apply_preset(&self, cx: &PluginContext<AriwaBassParams>, idx: usize) {
        let p = &FACTORY_PRESETS[idx];
        set_plain_param(cx, P::AriwaFxOn, if p.ariwa_fx_on { 1.0 } else { 0.0 });
        set_plain_param(cx, P::VoiceModel, p.voice_model as usize as f64);
        set_plain_param(cx, P::Cutoff, f64::from(p.cutoff_hz));
        set_plain_param(cx, P::Ronca, f64::from(p.ronca));
        set_plain_param(cx, P::Desliza, f64::from(p.desliza));
        set_plain_param(cx, P::Wobble, f64::from(p.wobble));
        set_plain_param(cx, P::Speed, f64::from(p.speed_hz));
        set_plain_param(cx, P::Sync, if p.sync { 1.0 } else { 0.0 });
        set_plain_param(cx, P::SyncDiv, p.sync_div as usize as f64);
        set_plain_param(cx, P::Slope, p.slope as usize as f64);
        set_plain_param(cx, P::Glide, f64::from(p.glide_ms));
        set_plain_param(cx, P::SubOct, f64::from(p.sub_oct));
        set_plain_param(cx, P::Decay, f64::from(p.decay_ms));
        set_plain_param(cx, P::Drive, f64::from(p.drive));
        set_plain_param(cx, P::Phaser, f64::from(p.phaser));
        set_plain_param(cx, P::Glue, f64::from(p.glue));
        set_plain_param(cx, P::OutGain, f64::from(p.out_gain_db));
    }

    fn draw_display_topbar(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        let painter = ui.painter();
        let tab_w = 82.0;
        for (i, (view, label)) in [
            (DisplayView::FilterPhaser, "Filter & Phaser"),
            (DisplayView::GlideActivity, "Glide & GR"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::from_min_size(
                pos2(rect.left() + i as f32 * (tab_w + 3.0), rect.top()),
                vec2(tab_w, rect.height()),
            );
            let resp = ui.interact(r, ui.id().with(("disp_tab", i)), Sense::click());
            if resp.clicked() {
                self.view = view;
            }
            let active = self.view == view;
            painter.rect_filled(r, 2.0, if active { ACCENT_FILTER } else { HEADER });
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.5),
                if active { Color32::BLACK } else { TEXT_DIM },
            );
        }

        // Right half: Akai MFC42 Slope Selector Pills (2P | 4P | 8P)
        let slope_id = u32::from(P::Slope);
        let cur_slope = cx.get_param_plain(slope_id).round() as usize;
        let pill_w = 32.0;
        let start_x = rect.right() - 3.0 * pill_w - 4.0;
        for (idx, (label, tip)) in [
            ("2P", "Akai MFC42 2-Pole (-12 dB/oct) SSM2164 SVF"),
            ("4P", "Akai MFC42 4-Pole (-24 dB/oct) Cascaded SVF"),
            ("8P", "Akai MFC42 8-Pole (-48 dB/oct) Mono Brickwall Growl"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::from_min_size(
                pos2(start_x + idx as f32 * (pill_w + 2.0), rect.top()),
                vec2(pill_w, rect.height()),
            );
            let resp = ui
                .interact(r, ui.id().with(("slope_pill", idx)), Sense::click())
                .on_hover_text(tip);
            if resp.clicked() {
                set_plain_param(cx, P::Slope, idx as f64);
            }
            let active = cur_slope == idx;
            painter.rect_filled(r, 2.0, if active { ACCENT_RONCA } else { HEADER });
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.5),
                if active { Color32::BLACK } else { TEXT_DIM },
            );
        }
    }

    fn draw_filter_phaser_view(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        let plot = Rect::from_min_max(
            pos2(rect.left() + 8.0, rect.top() + 8.0),
            pos2(rect.right() - 8.0, rect.bottom() - 30.0),
        );

        let to_x = |f: f32| plot.left() + freq_to_norm(f) * plot.width();
        let to_y = |db: f32| {
            plot.bottom()
                - ((db.clamp(PLOT_DB_MIN, PLOT_DB_MAX) - PLOT_DB_MIN)
                    / (PLOT_DB_MAX - PLOT_DB_MIN))
                    * plot.height()
        };

        // Logarithmic frequency grid lines
        for (f, lbl) in [
            (50.0, "50"),
            (100.0, "100"),
            (250.0, "250"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2500.0, "2.5k"),
            (6000.0, "6k"),
        ] {
            let x = to_x(f);
            painter.vline(x, plot.y_range(), Stroke::new(1.0_f32, GRID));
            painter.text(
                pos2(x, plot.bottom() + 2.0),
                Align2::CENTER_TOP,
                lbl,
                FontId::proportional(8.0),
                TEXT_DIM,
            );
        }

        // dB horizontal grid lines
        for db in [-24.0f32, -12.0, 0.0, 12.0] {
            let y = to_y(db);
            let col = if db == 0.0 { TRACK } else { GRID };
            painter.hline(plot.x_range(), y, Stroke::new(1.0_f32, col));
        }

        // Active MIDI note fundamental marker line
        if self.active_note > 0 {
            let nx = to_x(self.note_hz.clamp(FREQ_MIN, FREQ_MAX));
            painter.vline(
                nx,
                plot.y_range(),
                Stroke::new(1.2_f32, ACCENT_GLUE.gamma_multiply(0.55)),
            );
        }

        // Interactive X/Y Drag on Display (X = Cutoff, Y = Ronca)
        let drag_resp = ui
            .interact(plot, ui.id().with("xy_filter_pad"), Sense::click_and_drag())
            .on_hover_text(
                "Drag horizontally for Frequency (Cutoff), vertically for Ronca (Resonance & Diode Growl)",
            );
        if drag_resp.drag_started() {
            cx.begin_edit(P::Cutoff);
            cx.begin_edit(P::Ronca);
        }
        if drag_resp.dragged()
            && let Some(pos) = drag_resp.interact_pointer_pos()
        {
            let nx = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
            let ny = (1.0 - (pos.y - plot.top()) / plot.height()).clamp(0.0, 1.0);
            let new_fc = norm_to_freq(nx).clamp(35.0, 2500.0);
            set_plain_continuous(cx, P::Cutoff, f64::from(new_fc));
            set_plain_continuous(cx, P::Ronca, f64::from(ny * 100.0));
        }
        if drag_resp.drag_stopped() {
            cx.end_edit(P::Cutoff);
            cx.end_edit(P::Ronca);
        }

        let fx_on = cx.get_param(P::AriwaFxOn) > 0.5;
        let base_fc = cx.get_param_plain(P::Cutoff);
        let ronca = if fx_on {
            cx.get_param_plain(P::Ronca) * 0.01
        } else {
            0.12
        };
        let phaser_mix = if fx_on {
            cx.get_param_plain(P::Phaser) * 0.01
        } else {
            0.0
        };
        let slope = match cx.get_param_plain(P::Slope).round() as usize {
            0 => FilterSlope::TwoPole,
            1 => FilterSlope::FourPole,
            _ => FilterSlope::EightPole,
        };
        let live_fc = self.active_cutoff_hz.clamp(30.0, 8000.0);

        // 1. MXR 131 6-Stage Vactrol Phaser Comb Curve (MXR Blue trace)
        let n_pts = 96;
        if phaser_mix > 0.01 {
            let phaser_pts: Vec<Pos2> = (0..=n_pts)
                .map(|i| {
                    let norm = i as f32 / n_pts as f32;
                    let f = norm_to_freq(norm);
                    let db = crate::dsp::phaser_response_db(f, phaser_mix, self.phaser_f0_hz);
                    pos2(plot.left() + norm * plot.width(), to_y(db))
                })
                .collect();
            painter.line(
                phaser_pts,
                Stroke::new(1.3_f32, ACCENT_MXR.gamma_multiply(0.65)),
            );
        }

        // 2. Resting (Base) Cutoff Ghost Curve (dashed)
        let ghost_pts: Vec<Pos2> = (0..=n_pts)
            .map(|i| {
                let norm = i as f32 / n_pts as f32;
                let f = norm_to_freq(norm);
                let db = analytical_response_db(f, base_fc, ronca, slope, 0.0, self.phaser_f0_hz);
                pos2(plot.left() + norm * plot.width(), to_y(db))
            })
            .collect();
        painter.extend(Shape::dashed_line(
            &ghost_pts,
            Stroke::new(1.0_f32, ACCENT_FILTER.gamma_multiply(0.45)),
            3.0,
            3.0,
        ));

        // 3. Live Modulated Filter + MXR 131 Phaser Curve
        let curve_col = if fx_on { ACCENT_FILTER } else { ACCENT_GLUE };
        let live_pts: Vec<Pos2> = (0..=n_pts)
            .map(|i| {
                let norm = i as f32 / n_pts as f32;
                let f = norm_to_freq(norm);
                let db = analytical_response_db(
                    f,
                    live_fc,
                    ronca,
                    slope,
                    phaser_mix,
                    self.phaser_f0_hz,
                );
                pos2(plot.left() + norm * plot.width(), to_y(db))
            })
            .collect();

        for p in live_pts.iter().step_by(2) {
            painter.vline(
                p.x,
                p.y..=plot.bottom(),
                Stroke::new(2.0_f32, curve_col.gamma_multiply(0.10)),
            );
        }
        painter.line(live_pts, Stroke::new(2.2_f32, curve_col));

        // 4. Base Cutoff Handle & Live Sliding Orb ("Desliza")
        let base_peak_db =
            analytical_response_db(base_fc, base_fc, ronca, slope, 0.0, self.phaser_f0_hz);
        let base_pos = pos2(to_x(base_fc), to_y(base_peak_db));
        painter.circle_stroke(base_pos, 5.0, Stroke::new(1.5_f32, ACCENT_FILTER));

        let live_peak_db = analytical_response_db(
            live_fc,
            live_fc,
            ronca,
            slope,
            phaser_mix,
            self.phaser_f0_hz,
        );
        let live_pos = pos2(to_x(live_fc), to_y(live_peak_db));
        painter.circle_filled(live_pos, 4.5, ACCENT_SLIDE);
        painter.circle_stroke(live_pos, 7.0, Stroke::new(1.2_f32, ACCENT_RONCA));

        // Top-left active MIDI note badge inside plot
        let note_badge = if self.active_note > 0 {
            format!(
                "NOTE {} • {:.1} Hz",
                midi_note_name(self.active_note),
                self.note_hz
            )
        } else {
            "MIDI READY • CLICK KEYS BELOW".to_owned()
        };
        painter.text(
            pos2(plot.left() + 4.0, plot.top() + 2.0),
            Align2::LEFT_TOP,
            note_badge,
            FontId::proportional(8.5),
            if self.active_note > 0 {
                ACCENT_GLUE
            } else {
                TEXT_DIM
            },
        );

        // Bottom hardware telemetry bar inside display
        let slope_txt = match slope {
            FilterSlope::TwoPole => "MFC42 2P",
            FilterSlope::FourPole => "MFC42 4P",
            FilterSlope::EightPole => "MFC42 8P",
        };
        let desliza_val = cx.get_param_plain(P::Desliza);
        let mode_txt = if !fx_on {
            "CLEAN DUB SUB"
        } else if desliza_val < -1.0 {
            "PROG 41 TRON DOWN"
        } else if desliza_val > 1.0 {
            "MU-TRON UP GLIDE"
        } else {
            "KEY-TRACKED RESO"
        };
        painter.text(
            pos2(rect.left() + 8.0, rect.bottom() - 4.0),
            Align2::LEFT_BOTTOM,
            format!("{slope_txt} • {mode_txt}"),
            FontId::proportional(8.5),
            TEXT_DIM,
        );
        painter.text(
            pos2(rect.right() - 8.0, rect.bottom() - 4.0),
            Align2::RIGHT_BOTTOM,
            format!("fc {:.0} Hz", live_fc),
            FontId::proportional(9.0),
            ACCENT_SLIDE,
        );
    }

    fn draw_activity_view(&self, painter: &egui::Painter, rect: Rect) {
        let plot = rect.shrink(8.0);
        for i in 0..=4 {
            let y = plot.top() + (i as f32 / 4.0) * plot.height();
            painter.hline(plot.x_range(), y, Stroke::new(1.0_f32, GRID));
        }
        let n = self.history.len();
        if n >= 2 {
            let step = plot.width() / (HISTORY_LEN - 1) as f32;
            let x0 = plot.right() - step * (n - 1) as f32;

            // 1. Mu-Tron III Optocoupler Envelope as filled cyan-grey bars
            for (i, &(env, _, _)) in self.history.iter().enumerate() {
                let x = x0 + step * i as f32;
                let y = plot.bottom() - env.clamp(0.0, 1.0) * plot.height();
                painter.vline(
                    x,
                    y..=plot.bottom(),
                    Stroke::new(step.max(1.0), ACCENT_SLIDE.gamma_multiply(0.22)),
                );
            }

            // 2. Sliding Cutoff Trajectory (Amber line)
            let fc_line: Vec<Pos2> = self
                .history
                .iter()
                .enumerate()
                .map(|(i, &(_, fc_n, _))| {
                    pos2(
                        x0 + step * i as f32,
                        plot.bottom() - fc_n.clamp(0.0, 1.0) * plot.height(),
                    )
                })
                .collect();
            painter.line(fc_line, Stroke::new(2.0_f32, ACCENT_FILTER));

            // 3. LA-4 / dbx 166 Gain Reduction hanging from the top (Green line)
            let gr_line: Vec<Pos2> = self
                .history
                .iter()
                .enumerate()
                .map(|(i, &(_, _, gr))| {
                    pos2(
                        x0 + step * i as f32,
                        plot.top() + (gr / GR_RANGE).clamp(0.0, 1.0) * plot.height() * 0.65,
                    )
                })
                .collect();
            painter.line(gr_line, Stroke::new(1.6_f32, ACCENT_GLUE));
        }

        painter.text(
            plot.left_top(),
            Align2::LEFT_TOP,
            "LA-4 / dbx 166 GR",
            FontId::proportional(9.0),
            ACCENT_GLUE,
        );
        painter.text(
            pos2(plot.right(), plot.top()),
            Align2::RIGHT_TOP,
            "Cutoff Glide",
            FontId::proportional(9.0),
            ACCENT_FILTER,
        );
        painter.text(
            plot.left_bottom(),
            Align2::LEFT_BOTTOM,
            "Opto Envelope",
            FontId::proportional(9.0),
            ACCENT_SLIDE,
        );
    }

    fn draw_controls(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        centre: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        // --- BASS VOICE GENERATOR SELECTOR BAR ---
        let voice_bar_h = 18.0;
        let voice_rect = Rect::from_min_size(centre.min, vec2(centre.width(), voice_bar_h));
        painter.text(
            pos2(voice_rect.left(), voice_rect.center().y),
            Align2::LEFT_CENTER,
            "BASS VOICE",
            FontId::proportional(9.0),
            TEXT_DIM,
        );

        let cur_voice = cx.get_param_plain(P::VoiceModel).round() as usize;
        let pill_x0 = voice_rect.left() + 62.0;
        let pill_w = (voice_rect.right() - pill_x0 - 9.0) / 4.0;
        for (idx, (label, tip)) in [
            (
                "SH-101 Sub",
                "Roland SH-101 / Juno-60 monophonic UK dub synth bass (Saw + PWM Pulse + -1 Oct Sub)",
            ),
            (
                "Mini 3x",
                "Moog Minimoog Model D triple detuned analog oscillator stack (0, +7.5c, -7.5c)",
            ),
            (
                "Flatwound",
                "Plucked Fender Jazz/Precision flatwound electric bass physical string model",
            ),
            (
                "Steppers",
                "1990s UK Dub / Jah Shaka steppers bass (Pure Sine Sub + stereo detuned Saws)",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::from_min_size(
                pos2(pill_x0 + idx as f32 * (pill_w + 3.0), voice_rect.top()),
                vec2(pill_w, voice_bar_h),
            );
            let resp = ui
                .interact(r, ui.id().with(("voice_pill", idx)), Sense::click())
                .on_hover_text(tip);
            if resp.clicked() {
                set_plain_param(cx, P::VoiceModel, idx as f64);
            }
            let active = cur_voice == idx;
            painter.rect_filled(r, 2.0, if active { ACCENT_FILTER } else { HEADER });
            painter.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional(9.2),
                if active { Color32::BLACK } else { TEXT },
            );
        }

        // --- TOP ROW: 5 Main "Ronca e Desliza" Filter & Sweep Dials ---
        let row1_top = voice_rect.bottom() + 6.0;
        let top_h = 114.0;
        let main_w = centre.width() / 5.0;
        let sync_on = cx.get_param(P::Sync) > 0.5;

        let main_dials: [(P, &str, Color32, bool, &str); 5] = [
            (
                P::Cutoff,
                "Frequency",
                ACCENT_FILTER,
                false,
                "Key-tracked base cutoff frequency of the Akai MFC42 / Line 6 Filter Pro SVF",
            ),
            (
                P::Ronca,
                "Ronca",
                ACCENT_RONCA,
                false,
                "Coupled SSM2164 resonance peak + 1SS133 silicon diode throat growl",
            ),
            (
                P::Desliza,
                "Desliza",
                ACCENT_SLIDE,
                true,
                "Note-triggered Mu-Tron III CdS optocoupler envelope slide (< 0% = Program 41 Tron Down plunge; > 0% = Tron Up sweep)",
            ),
            (
                P::Wobble,
                "Wobble",
                ACCENT_SLIDE,
                false,
                "Optically smoothed Groove LFO modulation depth",
            ),
            (
                if sync_on { P::SyncDiv } else { P::Speed },
                if sync_on { "Division" } else { "Speed" },
                ACCENT_SLIDE,
                false,
                "LFO sweep speed and optocoupler release glide rate",
            ),
        ];

        for (i, (id, label, col, bipolar, tip)) in main_dials.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(centre.left() + main_w * i as f32, row1_top),
                vec2(main_w, top_h),
            );
            dial(ui, cx, r, id, label, 20.0, col, bipolar, tip);
        }

        // Compact SYNC toggle button right underneath the Speed/Division dial
        let sync_rect = Rect::from_center_size(
            pos2(
                centre.left() + main_w * 4.5,
                row1_top + top_h - 6.0,
            ),
            vec2(46.0, 14.0),
        );
        toggle_btn(
            ui,
            cx,
            sync_rect,
            P::Sync,
            "SYNC",
            ACCENT_SLIDE,
            "Lock LFO wobble speed to host BPM tempo",
        );

        // --- Horizontal Divider ---
        let sep_y = row1_top + top_h + 4.0;
        painter.hline(centre.x_range(), sep_y, Stroke::new(1.0_f32, GRID));

        // --- BOTTOM ROW: 7 Bass Synth & Ariwa Rack Dials ---
        let row2_top = sep_y + 6.0;
        let row2_h = centre.bottom() - row2_top;
        let sub_w = centre.width() / 7.0;

        let rack_dials: [(P, &str, Color32, bool, &str); 7] = [
            (
                P::Glide,
                "Glide",
                ACCENT_SLIDE,
                false,
                "Analog RC pitch portamento / legato note slide time (0 to 500 ms)",
            ),
            (
                P::SubOct,
                "Sub Oct",
                ACCENT_MXR,
                false,
                "MXR Model 129 Pitch Transposer -1 Octave companded sub-bass level",
            ),
            (
                P::Decay,
                "Decay",
                ACCENT_FILTER,
                false,
                "Bass pluck & sustain contour decay time (80 ms to 2.5 s)",
            ),
            (
                P::Drive,
                "Drive",
                ACCENT_DRIVE,
                false,
                "Otari MTR-90 2\" tape head bump (62 Hz) + Faraday 1/f transformer flux & diode saturation",
            ),
            (
                P::Phaser,
                "Phaser",
                ACCENT_MXR,
                false,
                "Dual MXR Series 131 (M-110) 6-Stage Vactrol Auto Phaser depth above 95 Hz",
            ),
            (
                P::Glue,
                "Glue",
                ACCENT_GLUE,
                false,
                "UREI LA-4 optical leveler + dbx 166 Blackmer True-RMS VCA & PeakStop ceiling limiter",
            ),
            (
                P::OutGain,
                "Out Gain",
                ACCENT_FILTER,
                true,
                "Final output level trim (-24 dB to +24 dB)",
            ),
        ];

        for (i, (id, label, col, bipolar, tip)) in rack_dials.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(centre.left() + sub_w * i as f32, row2_top),
                vec2(sub_w, row2_h),
            );
            dial(ui, cx, r, id, label, 15.0, col, bipolar, tip);
        }
    }

    fn draw_keyboard(
        &mut self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        cx: &PluginContext<AriwaBassParams>,
    ) {
        painter.rect_filled(rect, 2.0, DISPLAY);
        painter.rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0_f32, GRID),
            StrokeKind::Inside,
        );

        // 2-Octave Bass Range: C1 (MIDI 24) to C3 (MIDI 48) -> 15 white keys, 10 black keys
        const START_NOTE: u8 = 24;
        const END_NOTE: u8 = 48;
        const IS_BLACK: [bool; 12] = [
            false, true, false, true, false, false, true, false, true, false, true, false,
        ];

        let inner = rect.shrink(2.0);
        let white_count = 15usize;
        let white_w = inner.width() / white_count as f32;

        // Build white & black key rectangles so black keys take hit-test priority
        let mut white_keys: [(u8, Rect); 15] = [(0, Rect::NOTHING); 15];
        let mut black_keys: [(u8, Rect); 10] = [(0, Rect::NOTHING); 10];
        let mut w_idx = 0usize;
        let mut b_idx = 0usize;

        for note in START_NOTE..=END_NOTE {
            let semi = (note % 12) as usize;
            if !IS_BLACK[semi] {
                let x = inner.left() + w_idx as f32 * white_w;
                let r = Rect::from_min_size(pos2(x, inner.top()), vec2(white_w - 1.0, inner.height()));
                white_keys[w_idx] = (note, r);
                w_idx += 1;
            } else {
                let x_center = inner.left() + w_idx as f32 * white_w;
                let bw = white_w * 0.62;
                let r = Rect::from_min_size(
                    pos2(x_center - bw * 0.5, inner.top()),
                    vec2(bw, inner.height() * 0.62),
                );
                black_keys[b_idx] = (note, r);
                b_idx += 1;
            }
        }

        // Interactive click & drag auditioning across the entire keyboard strip
        let kbd_resp = ui
            .interact(inner, ui.id().with("ariwa_bass_kbd"), Sense::click_and_drag())
            .on_hover_text("Click and drag to audition Ariwa Dub Bass notes and legato slides (C1 – C3)");

        let pointer_down = kbd_resp.is_pointer_button_down_on();
        let mut hovered_note = 0u8;
        if let Some(pos) = kbd_resp.interact_pointer_pos()
            && inner.contains(pos)
        {
            for &(note, r) in &black_keys {
                if r.contains(pos) {
                    hovered_note = note;
                    break;
                }
            }
            if hovered_note == 0 {
                for &(note, r) in &white_keys {
                    if r.contains(pos) {
                        hovered_note = note;
                        break;
                    }
                }
            }
        }

        let target_gui_note = if pointer_down { hovered_note } else { 0 };
        if target_gui_note != self.gui_pressed_note {
            self.gui_pressed_note = target_gui_note;
            set_plain_param(cx, P::GuiNote, f64::from(target_gui_note));
        }

        let sounding_note = if self.active_note > 0 {
            self.active_note
        } else {
            self.gui_pressed_note
        };

        // Paint white keys
        for &(note, r) in &white_keys {
            let is_active = sounding_note == note;
            let fill = if is_active {
                ACCENT_FILTER
            } else {
                Color32::from_rgb(72, 72, 76)
            };
            painter.rect_filled(r, 1.5, fill);
            if note.is_multiple_of(12) {
                painter.text(
                    pos2(r.center().x, r.bottom() - 2.0),
                    Align2::CENTER_BOTTOM,
                    midi_note_name(note),
                    FontId::proportional(8.5),
                    if is_active { Color32::BLACK } else { TEXT },
                );
            }
        }

        // Paint black keys on top
        for &(note, r) in &black_keys {
            let is_active = sounding_note == note;
            let fill = if is_active {
                ACCENT_SLIDE
            } else {
                Color32::from_rgb(26, 26, 28)
            };
            painter.rect_filled(r, 1.5, fill);
            painter.rect_stroke(
                r,
                1.5,
                Stroke::new(1.0_f32, Color32::from_rgb(14, 14, 16)),
                StrokeKind::Inside,
            );
        }
    }

    fn draw_meters(&self, painter: &egui::Painter, rect: Rect) {
        let label_h = 14.0;
        let bars = Rect::from_min_max(
            pos2(rect.left(), rect.top() + label_h),
            pos2(rect.right(), rect.bottom() - label_h),
        );
        let bar_w = 7.0;

        // 1. Gain Reduction pair (top-down)
        let gr_x = rect.left() + 4.0;
        painter.text(
            pos2(gr_x + bar_w + 1.0, rect.top()),
            Align2::CENTER_TOP,
            "GR",
            FontId::proportional(9.0),
            TEXT_DIM,
        );
        for ch in 0..2 {
            let r = Rect::from_min_size(
                pos2(gr_x + ch as f32 * (bar_w + 2.0), bars.top()),
                vec2(bar_w, bars.height()),
            );
            painter.rect_filled(r, 0.0, DISPLAY);
            let h = (self.gr_db[ch] / GR_RANGE).clamp(0.0, 1.0) * r.height();
            painter.rect_filled(Rect::from_min_size(r.min, vec2(bar_w, h)), 0.0, ACCENT_GLUE);
        }
        let gr = self.gr_db[0].max(self.gr_db[1]);
        painter.text(
            pos2(gr_x + bar_w + 1.0, rect.bottom()),
            Align2::CENTER_BOTTOM,
            format!("-{gr:.1}"),
            FontId::proportional(9.0),
            TEXT,
        );

        // 2. Output Peak pair (bottom-up, -60..0 dBFS)
        let out_x = rect.right() - 2.0 * bar_w - 6.0;
        painter.text(
            pos2(out_x + bar_w + 1.0, rect.top()),
            Align2::CENTER_TOP,
            "Out",
            FontId::proportional(9.0),
            TEXT_DIM,
        );
        for ch in 0..2 {
            let r = Rect::from_min_size(
                pos2(out_x + ch as f32 * (bar_w + 2.0), bars.top()),
                vec2(bar_w, bars.height()),
            );
            painter.rect_filled(r, 0.0, DISPLAY);
            let level = ((self.out_peak_db[ch] - DB_MIN) / (-DB_MIN)).clamp(0.0, 1.0);
            let segments = [
                (0.0, 0.80, METER_LOW),
                (0.80, 0.95, METER_MID),
                (0.95, 1.00, METER_HIGH),
            ];
            for (lo, hi, col) in segments {
                if level > lo {
                    let top = r.bottom() - level.min(hi) * r.height();
                    let bottom = r.bottom() - lo * r.height();
                    painter.rect_filled(
                        Rect::from_min_max(pos2(r.left(), top), pos2(r.right(), bottom)),
                        0.0,
                        col,
                    );
                }
            }
        }

        // dB scale between GR and Out bars
        for d in [0i32, -12, -24, -48] {
            let y = bars.bottom() - (d as f32 - DB_MIN) / (-DB_MIN) * bars.height();
            painter.text(
                pos2(out_x - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{}", d.abs()),
                FontId::proportional(8.0),
                TEXT_DIM,
            );
        }

        let peak = self.out_peak_db[0].max(self.out_peak_db[1]);
        let peak_text = if peak <= DB_MIN {
            "-inf".to_owned()
        } else {
            format!("{peak:.1}")
        };
        painter.text(
            pos2(out_x + bar_w + 1.0, rect.bottom()),
            Align2::CENTER_BOTTOM,
            peak_text,
            FontId::proportional(9.0),
            if peak > -0.1 { METER_HIGH } else { TEXT },
        );
    }
}

fn param_default_normalized(cx: &PluginContext<AriwaBassParams>, id: u32) -> Option<f64> {
    cx.params()
        .param_infos()
        .into_iter()
        .find(|i| i.id == id)
        .map(|i| i.range.normalize(i.default_plain))
}

fn set_plain_param(cx: &PluginContext<AriwaBassParams>, id: impl Into<u32>, plain: f64) {
    let id = id.into();
    if let Some(info) = cx.params().param_infos().into_iter().find(|i| i.id == id) {
        let norm = info.range.normalize(plain);
        cx.begin_edit(id);
        cx.set_param(id, norm);
        cx.end_edit(id);
    }
}

fn set_plain_continuous(cx: &PluginContext<AriwaBassParams>, id: impl Into<u32>, plain: f64) {
    let id = id.into();
    if let Some(info) = cx.params().param_infos().into_iter().find(|i| i.id == id) {
        let norm = info.range.normalize(plain);
        cx.set_param(id, norm);
    }
}

/// Ableton Live-style thin-arc dial: label above, 270° arc with indicator needle, formatted value below.
#[allow(clippy::too_many_arguments)]
fn dial(
    ui: &mut egui::Ui,
    cx: &PluginContext<AriwaBassParams>,
    rect: Rect,
    id: impl Into<u32>,
    label: &str,
    radius: f32,
    accent: Color32,
    bipolar: bool,
    tooltip: &str,
) {
    let id: u32 = id.into();
    let centre = pos2(rect.center().x, rect.top() + 14.0 + radius + 3.0);
    let hit = Rect::from_center_size(centre, Vec2::splat(radius * 2.0 + 10.0));
    let resp = ui
        .interact(hit, ui.id().with(("ariwa_dial", id)), Sense::click_and_drag())
        .on_hover_text(tooltip);

    let mut value = cx.get_param(id);
    if resp.drag_started() {
        cx.begin_edit(id);
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let delta = -resp.drag_delta().y / if fine { 1800.0 } else { 190.0 };
        if delta != 0.0 {
            value = (value + delta).clamp(0.0, 1.0);
            cx.set_param(id, f64::from(value));
        }
    }
    if resp.drag_stopped() {
        cx.end_edit(id);
    }
    if resp.double_clicked()
        && let Some(d) = param_default_normalized(cx, id)
    {
        cx.begin_edit(id);
        cx.set_param(id, d);
        cx.end_edit(id);
        value = d as f32;
    }

    let painter = ui.painter();
    let active = resp.hovered() || resp.dragged();
    painter.text(
        pos2(rect.center().x, rect.top()),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(10.0),
        if active { TEXT } else { TEXT_DIM },
    );

    const START: f32 = std::f32::consts::PI * 0.75;
    const SWEEP: f32 = std::f32::consts::PI * 1.5;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let n = ((to - from).abs() * 24.0).ceil().max(2.0) as usize;
        (0..=n)
            .map(|i| {
                let a = START + SWEEP * (from + (to - from) * i as f32 / n as f32);
                centre + vec2(a.cos(), a.sin()) * radius
            })
            .collect()
    };

    let stroke_w: f32 = if radius > 18.0 { 3.0 } else { 2.5 };
    painter.line(arc(0.0, 1.0), Stroke::new(stroke_w, TRACK));
    if bipolar {
        let (a0, a1) = (0.5f32.min(value), 0.5f32.max(value));
        if (a1 - a0) > 0.005 {
            painter.line(arc(a0, a1), Stroke::new(stroke_w, accent));
        }
    } else if value > 0.002 {
        painter.line(arc(0.0, value), Stroke::new(stroke_w, accent));
    }

    let a = START + SWEEP * value;
    let dir = vec2(a.cos(), a.sin());
    painter.line_segment(
        [centre + dir * (radius * 0.25), centre + dir * radius],
        Stroke::new(1.8_f32, if active { Color32::WHITE } else { TEXT }),
    );

    painter.text(
        pos2(rect.center().x, centre.y + radius + 3.0),
        Align2::CENTER_TOP,
        cx.format_param(id),
        FontId::proportional(if radius > 18.0 { 10.5 } else { 9.0 }),
        if active { accent } else { TEXT },
    );
}

fn toggle_btn(
    ui: &mut egui::Ui,
    cx: &PluginContext<AriwaBassParams>,
    rect: Rect,
    id: impl Into<u32>,
    label: &str,
    accent: Color32,
    tip: &str,
) {
    let id: u32 = id.into();
    let resp = ui
        .interact(rect, ui.id().with(("ariwa_toggle", id)), Sense::click())
        .on_hover_text(tip);
    let on = cx.get_param(id) > 0.5;
    if resp.clicked() {
        cx.begin_edit(id);
        cx.set_param(id, if on { 0.0 } else { 1.0 });
        cx.end_edit(id);
    }
    let on = cx.get_param(id) > 0.5;
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, if on { accent } else { PANEL });
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0_f32, if resp.hovered() { TEXT_DIM } else { TRACK }),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(9.5),
        if on { Color32::BLACK } else { TEXT_DIM },
    );
}
