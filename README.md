# Ariwa Bass —  Dub Bass Instrument

[![CI](https://github.com/nagualcode/ariwabass/actions/workflows/ci.yml/badge.svg)](https://github.com/nagualcode/ariwabass)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Format](https://img.shields.io/badge/Plugin%20Formats-VST3%20%7C%20CLAP%20%7C%20AU%20%7C%20Standalone-orange.svg)](#installation)

**Ariwa Bass** is a zero-allocation, real-time analog modeling **Dub Bass Instrument (VST3, CLAP, Audio Unit, and Standalone)** built in Rust using the [Truce](https://github.com/truce-audio/truce) framework.



## 🎹 Synthesizer Architecture & Dynamic Bass Containment

 **Ariwa Bass** incorporates containment principles from classic bass synthesizer patents (**Roland TB-303 / Tadao Kikumoto US Patent 4,635,521** and Bob Moog's Minimoog):

1. **Contained VCA & VCF Decay Envelopes:**
   - Ultra-fast $1.8\text{ ms}$ click-free analog attack.
   - Exponential decay contour ($80\text{ ms}$ to $2.5\text{ s}$) with a low, controlled sustain floor ($\le 0.25$), preventing notes from turning into endless droning organs.
   - Snappy $25\text{ ms}$ release on `NoteOff`, cutting off any filter ring or feedback immediately.
2. **Dynamic Resonance Damping (`Ronca`):**
   - Resonant Q peak ($Q \le 5.2$) bites on note attack and decays smoothly with the note, preventing endless whistling self-oscillation while notes are held.
   - **By default, all factory presets are configured with `Ronca` $\le 10\%$**, ensuring clean, round, punchy sub-bass out of the box.
3. **Note-Triggered Synchronized Wobble LFO:**
   - LFO phase resets on every `NoteOn` for perfect rhythmic alignment.
   - Modulation depth is scaled by the note envelope, allowing subtle, musical dub movement ($\pm 0.55$ octaves) that settles naturally.
4. **4 Dedicated Bass Voice Models:**
   - **`SH-101 Sub`**: Roland SH-101 / Juno-60 PolyBLEP Sawtooth + Pulse + phase-locked $-1\text{ Oct}$ Sub.
   - **`Mini 3x`**: Triple detuned Moog Minimoog Model D oscillator stack ($0$, $+7.5$, $-7.5$ cents).
   - **`Flatwound`**: Physical model of a plucked Fender Jazz/Precision bass with flatwound strings.
   - **`Steppers`**: 1990s UK Dub / Jah Shaka steppers bass (pure phase-locked Sine sub + detuned Saws).

---

## 🎛️ Parameters & Interface (Ableton Live Inspired)

| Parameter | Range | Description |
| :--- | :--- | :--- |
| **`RONCA FX: ON / CLEAN SUB`** | Toggle | Master switch for the "Ronca e Desliza" effect chain (Clean Dub Sub when Off). |
| **`Bass Voice`** | 4 Models | Selects between `SH-101 Sub`, `Mini 3x`, `Flatwound`, or `Steppers`. |
| **`Frequency`** | $35\text{ Hz} - 2500\text{ Hz}$ | Key-tracked base cutoff frequency of the Akai MFC42 SVF (interactive on X-axis). |
| **`Ronca`** | $0\% - 100\%$ (Default $\le 10\%$) | Coupled SSM2164 resonance + 1SS133 silicon diode throat growl (interactive on Y-axis). |
| **`Desliza`** | $-100\% \text{ to } +100\%$ | Bipolar Mu-Tron III opto envelope (<0% Tron Down plunge; >0% Tron Up sweep). |
| **`Wobble`** | $0\% - 100\%$ | Optically smoothed Groove LFO modulation depth. |
| **`Speed / Division`** | $0.08\text{ Hz} - 12\text{ Hz}$ | LFO rate (or tempo division `1/1` to `1/16` when **`SYNC`** is engaged). |
| **`2P / 4P / 8P`** | Selector | Filter slope: 2-Pole ($-12\text{ dB}$), 4-Pole ($-24\text{ dB}$), or 8-Pole ($-48\text{ dB/oct}$). |
| **`Glide`** | $0 - 500\text{ ms}$ | Analog RC pitch portamento / legato note slide time. |
| **`Sub Oct`** | $0\% - 100\%$ | MXR Model 129 companded $-1\text{ Octave}$ sub-bass level. |
| **`Decay`** | $80 - 2500\text{ ms}$ | Bass pluck & sustain contour decay time. |
| **`Drive`** | $0\% - 100\%$ | Otari MTR-90 2" tape head bump ($62\text{ Hz}$) + Faraday $1/f$ flux saturation. |
| **`Phaser`** | $0\% - 100\%$ | Dual MXR Series 131 6-stage Vactrol auto phaser depth above $95\text{ Hz}$. |
| **`Glue`** | $0\% - 100\%$ | UREI LA-4 optical leveler + dbx 166 True-RMS VCA & PeakStop limiter. |
| **`Out Gain`** | $-24\text{ dB to } +24\text{ dB}$ | Final output level trim. |
| **`HQ 4x`** | Toggle | 4x linear-phase Kaiser oversampling on the 1SS133 diode growl stage. |
| **`Keyboard`** | 2 Octaves (`C1`–`C3`) | On-screen interactive keyboard for direct auditioning and MIDI note feedback. |

---

## 💾 Installation

### Automated Installation (macOS)

1. Download the release archive `Ariwa-Bass-v1.0.0-macOS-AppleSilicon.zip` from the [Releases](https://github.com/nagualcode/ariwabass/releases) page.
2. Unzip the archive.
3. In Terminal, navigate to the unzipped folder and run:
   ```bash
   chmod +x install.sh
   ./install.sh
   ```
4. Restart your DAW (Ableton Live, Logic Pro, Reaper, Bitwig, Studio One) to scan the new instrument.

### Manual Installation (macOS)

Copy the plugin bundles into your user Audio Plug-Ins folder:

* **VST3**: Copy `Ariwa Bass.vst3` to `~/Library/Audio/Plug-Ins/VST3/`
* **CLAP**: Copy `Ariwa Bass.clap` to `~/Library/Audio/Plug-Ins/CLAP/`
* **Audio Unit**: Copy `Ariwa Bass.component` to `~/Library/Audio/Plug-Ins/Components/`

---

## 🗑️ Uninstallation

Run the provided uninstall script:
```bash
chmod +x uninstall.sh
./uninstall.sh
```

Or manually remove the files:
```bash
rm -rf "$HOME/Library/Audio/Plug-Ins/VST3/Ariwa Bass.vst3"
rm -rf "$HOME/Library/Audio/Plug-Ins/CLAP/Ariwa Bass.clap"
rm -rf "$HOME/Library/Audio/Plug-Ins/Components/Ariwa Bass.component"
```

---

## 🛠️ Building from Source

Requires [Rust](https://rustup.rs/) (1.80+) and [Cargo Truce](https://github.com/truce-audio/truce):

```bash
# Install the Truce plugin build tool
cargo install truce-build --bin cargo-truce

# Clone the repository
git clone https://github.com/nagualcode/ariwabass.git
cd ariwabass

# Run the DSP test suite, harmonic validation, and GUI screenshot regression tests
cargo test

# Build optimized and signed VST3, CLAP, and AU bundles in target/bundles/
cargo truce build

# Install directly to your user plugin library
cargo truce install --user

# Run standalone application for direct testing
cargo run
```

---

## 📄 License

Distributed under the [MIT License](LICENSE).
Copyright (c) 2026 nagualcode.
