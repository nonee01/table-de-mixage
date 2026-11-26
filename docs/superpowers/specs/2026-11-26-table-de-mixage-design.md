# Table de mixage: design

Date: 2026-11-26. Demo: Tuesday 2026-12-01, in the Traitement de signal (PEI121) class.

## Intent

The PEI121 teacher told Yassine that if he builds a working mixing console program ("table de mixage")
that suppresses noise, so that a scream into the microphone does not come out, he will reward him.
The program is shown live on Tuesday on Yassine's laptop, and the teacher keeps a copy for his own
Windows PC. It should show the course at work: sampling, FIR filters, spectra in dB.

Success on Tuesday: Yassine talks into his earbuds or his phone, the voice comes out clean; he screams,
and depending on which block is switched on the scream comes out at talking level or not at all; a fan
or hum in the room disappears when Anti-bruit is ticked; the plot shows each of these happening.

## Goals

- Five input channels mixed live to one output, with low enough delay that talking into it feels live
  (target under 50 ms mic to ear, block of 256 samples at 48 kHz).
- Three noise blocks per channel, each with its own on/off check box: Anti-bruit, Coupe-cri, Limiteur.
- A plot of the selected channel before and after its processing, in time and as a spectrum in dB.
- Runs on Windows 10/11 x64 from an unzipped folder, nothing to install. Also builds and runs on Linux
  (Yassine's asus) for development.
- French interface.

## Non-goals (for Tuesday)

- The board version. It comes later (ESP32 or similar); for now only keep the processing code portable.
- Effects beyond the three blocks and the EQ (reverb, echo, pitch, etc.).
- Saving and loading mixer setups, MIDI, plugins, multitrack recording.
- An installer, code signing, auto-update.

## Anti-goals

- A dark "dashboard" look with cards, gradients, pills or emoji. Yassine rejected that as AI-made.
  The look is a plain Windows engineering tool (see Look).
- Crackles or dropouts during the demo. The audio thread never waits on the window.
- Requiring internet, accounts, or admin rights on the teacher's PC.

## Constraints

- Four working days: Thursday 26 to Monday 30 November, demo Tuesday morning.
- Inputs available: laptop mic or wired earbuds mic, and Yassine's Redmi phone over USB.
  Bluetooth earbuds would drop to 16 kHz hands-free mode on Windows; wired earbuds are the safe choice.
- Feedback (Larsen) risk: an open mic plus laptop speakers will howl. The demo listens on earbuds or
  headphones; if speakers are used for the room, keep the mic (the phone) away from them and the
  master limiter on.
- Build machine is Linux (asus). Windows builds use `cargo xwin` with the `x86_64-pc-windows-msvc`
  target, both already installed. Windows testing on hp-win through laptop-link.
- Piper on this laptop is the Python `piper-tts`; the French voice `fr_FR-siwis-medium` is not present
  here. Windows needs the standalone `piper.exe` release plus the voice files in the zip.
- Android applies its own noise reduction and gain control to the normal mic source. Use scrcpy's
  `mic-unprocessed` source so our Anti-bruit is what the teacher hears working.

## Approaches considered

1. **Rust native app (chosen).** egui window, cpal for audio, rustfft for spectra and Anti-bruit.
   One `.exe` around 10 MB, no runtime to install, the audio callback runs on its own thread so the
   window cannot cause dropouts, and the processing crate can later be compiled for a board.
   Costs: the TP filter code (about 50 lines of numpy) is rewritten in Rust; the UI is written from
   scratch.
2. **New Python app (Qt, pyqtgraph, sounddevice).** Reuses the TP code directly and is faster to write,
   but ships as a ~150 MB folder or needs Python on the teacher's PC, and the window and the audio share
   one interpreter, so redraws can cause crackles. Rejected because it must run on Windows without
   install and must not crackle.
3. **A tab inside the TP `simulation.py`.** One app, but grows a 1455-line file and mixes live audio
   with the TP graphs right before the demo. Rejected.
4. **Web page with the browser's audio engine.** Runs anywhere, but none of the Python work carries
   over, Piper is heavy in a browser, and nothing moves to a board. Rejected.

Yassine picked approach 1 ("Rust (Recommended)"), with the requirement that it runs on Windows.

## Decided by Yassine

- Name: **Table de mixage** (window title "Table de mixage", `table-de-mixage.exe`, project folder
  `~/Projects/school/table-de-mixage`).
- Processing runs on the laptop now; the board comes later.
- All three noise blocks, each switchable: Limiteur, Coupe-cri, Anti-bruit.
- Channels: Micro 1, Micro 2 (second live mic, the phone), Musique (file player), Voix (Piper
  text-to-speech), Bruit (test noise generator).
- Runs on his own laptop for the demo and on the teacher's PC (he keeps a copy).
- Look: mockup 3, "Plain engineering software", alone (not combined with the hardware desk).
  Reference: `.superpowers/brainstorm/2376265-1795651105/content/style.html`, third card.

## Look

A plain Windows tool, like Audacity, MATLAB or the Arduino IDE. Light grey window, no decoration.

```
 Fichier  Audio  Affichage  Aide                      48 000 Hz · bloc 256 · latence 11 ms
 ┌1. Micro 1──┐┌2. Micro 2──┐┌3. Musique──┐┌4. Voix─────┐┌5. Bruit────┐┌Sortie──────┐
 │[écouteurs▾]││[téléphone▾]││[chanson ▾] ││[Piper    ▾]││[50 Hz    ▾]││[Casque   ▾]│
 │ +6 ┃  ▮     ││ +6 ┃  ▮    ││ +6 ┃  ▮    ││ +6 ┃  ▮    ││ +6 ┃       ││ +6 ┃  ▮ ▮  │
 │  0 ╋  ▮     ││  0 ╋  ▮    ││  0 ┃  ▮    ││  0 ╋       ││  0 ┃       ││  0 ╋  ▮ ▮  │
 │-20 ┃  ▮     ││-20 ┃  ▮    ││-20 ╋  ▮    ││-20 ┃  ▮    ││-20 ╋       ││-20 ┃  ▮ ▮  │
 │Volume[-3 dB]││Volume[-6 dB]││...         ││...         ││...         ││Volume[0 dB]│
 │Graves Méd. Aigus (3 small sliders)        ││            ││            ││☑ Limiteur  │
 │☑ Anti-bruit││☑ Anti-bruit││☐ Anti-bruit││☐ Anti-bruit││☐ Anti-bruit││☐ Enregistrer│
 │☑ Coupe-cri ││☐ Coupe-cri ││☐ Coupe-cri ││☐ Coupe-cri ││☐ Coupe-cri ││            │
 │☑ Limiteur  ││☑ Limiteur  ││☑ Limiteur  ││☐ Limiteur  ││☐ Limiteur  ││            │
 │☐ Muet      ││☐ Muet      ││☐ Muet      ││☐ Muet      ││☑ Muet      ││            │
 └────────────┘└────────────┘└────────────┘└────────────┘└────────────┘└────────────┘
 ┌──────────────────────────────────────────────────────────────────────────────────┐
 │ white plot, black axes, light grid, ticks in Hz / ms, legend: ─ entrée  ─ sortie │
 └──────────────────────────────────────────────────────────────────────────────────┘
```

- Background `#f0f0f0`, group boxes with a thin `#c8c8c8` border and the title cut into the top edge,
  standard check boxes, combo boxes, numeric fields with units ("-6.0 dB", "3000 Hz").
- Font: Segoe UI on Windows (loaded from `C:\Windows\Fonts\segoeui.ttf` at start), a bundled
  DejaVu Sans fallback elsewhere. 11 px body.
- Vertical volume slider per channel with dB ticks (+6, 0, -20, -40, -∞) and a green level meter
  beside it; the output has a stereo pair of meters.
- Plot area: white, black axes, `#e6e6e6` grid, matplotlib default colours: entrée `#1f77b4` (blue),
  sortie `#ff7f0e` (orange). Two views, switchable in the plot header: "Signal x(t)" (last 200 ms) and
  "Spectre |X(f)| en dB" (0 to 24 kHz). It shows the channel whose box was clicked last (the selected
  box gets a slightly darker border).
- Menus: Fichier (Ouvrir une musique, Dossier des enregistrements, Quitter), Audio (choose devices,
  block size), Affichage (Signal / Spectre), Aide (À propos, short help on each block with its formula).
- Status text right of the menu bar: sample rate, block size, measured latency.

## Channels

| # | Name | Source | Notes |
|---|------|--------|-------|
| 1 | Micro 1 | any input device (default: system default mic) | wired earbuds or laptop mic |
| 2 | Micro 2 | the phone over USB via scrcpy, or any other input device | see Phone microphone |
| 3 | Musique | MP3 / WAV / FLAC file | Ouvrir, lecture/pause, boucle |
| 4 | Voix | Piper speaks the text typed in a field | "Dire" button, plays once |
| 5 | Bruit | generator: bruit blanc, ronflement 50 Hz (with harmonics), souffle (pink noise) | for showing Anti-bruit |

Mics, Voix and Bruit are mono and sit in the centre of the stereo output; Musique stays stereo (its
strip runs on left and right with shared settings). No pan control.

The plot's "entrée" is the channel's source signal, its "sortie" is after EQ and before the volume
slider, so muting or turning a channel down never hides what the blocks did. On the Signal view the
Limiteur threshold S and the Coupe-cri threshold T are drawn as dashed horizontal lines, so the teacher
sees the scream cross them. (Claude's calls, made while checking the design.)

Every channel has: source selector, volume slider (-∞ to +6 dB) with meter and dB field, a 3-band EQ
(Graves, Médiums, Aigus, ±12 dB), check boxes Anti-bruit, Coupe-cri, Limiteur, Muet.
Processing order in a channel: source → Anti-bruit → Coupe-cri → Limiteur → EQ → volume → mix.

Output ("Sortie"): device selector, master volume, its own Limiteur (on by default so nothing ever
clips), Enregistrer writes the master to WAV (16-bit, 48 kHz, stereo) in
`Documents/Table de mixage/enregistrement-AAAAMMJJ-HHMMSS.wav`.

## The three noise blocks

Each block has its parameters in the Audio menu or a small "Réglages..." link in the channel box;
defaults below must work for the demo without touching anything.

**Limiteur** (scream comes out quiet). Peak limiter. Envelope follower on |x| with attack 1 ms and
release 150 ms; gain g = min(1, S / env); output y = g·x. Threshold S default -12 dBFS. A 2 ms
look-ahead delay is allowed so the scream's first peak is caught. Guarantee: |y| never exceeds S
(plus a small tolerance).

**Coupe-cri** (scream does not come out). Level-triggered mute with a 20 ms look-ahead: the audio
is delayed 20 ms while the detector reads the undelayed signal, so the mute is in place before the
scream reaches the output. Short-term RMS over 10 ms. When it stays above threshold T (default
-10 dBFS) for 5 ms, the gain fades to 0 in 5 ms, holds 300 ms after the level falls back below T,
then fades back in over 50 ms. (Changed 2026-11-26 while planning: with no look-ahead the first
20 ms of every scream got through.) Normal talking at a sensible input gain
stays under T and passes untouched. The teacher should be able to move T.

**Anti-bruit** (background noise removed, voice kept). Two stages:
1. Spectral subtraction on an STFT (frame 512, Hann, 50 % overlap, so about 11 ms of added delay at
   48 kHz and 94 Hz per bin; 512 rather than 1024 keeps the whole chain under the 50 ms target). Noise magnitude profile N(f) is learned with an "Apprendre le bruit" button (records 1 s
   of the room with nobody talking), and also updated slowly from frames that look like silence.
   |Y| = max(|X| - α·N, β·|X|), α = 2, β = 0.05, phase of X kept, overlap-add back.
2. Noise gate after it: below -50 dBFS RMS the signal is attenuated by 30 dB (attack 5 ms, release
   100 ms), so silence is really silent.

## EQ (Graves / Médiums / Aigus)

Built from the course filters, so the teacher recognises them: windowed sinc with Hamming as in the TP
(`coefficients_du_filtre` in `TP/Simulation - Filtre passe-bas numérique/simulation.py`):
low-pass at 250 Hz (Graves), band-pass 250 Hz to 4 kHz (Médiums), high-pass at 4 kHz (Aigus). Because
the three are linear phase with the same length, one FIR per channel does the whole EQ:
h = g_G·h_PB + g_M·h_BP + g_A·h_PH, recomputed when a slider moves and swapped in at a block boundary.
Length 511 taps (about 5 ms of delay), applied by FFT overlap-add; when all three gains are 0 dB the
EQ is skipped and adds no delay. Delay budget with every block on: device and buffers about 16 ms,
Limiteur 2 ms, Coupe-cri 20 ms, Anti-bruit 11 ms, EQ 5 ms, about 54 ms; with the EQ flat, under 50 ms. (This was Claude's proposal; Yassine has not
said no to it. Dropping it is allowed if time runs short; nothing else depends on it.)

## Phone microphone

Micro 2 can read the phone's microphone over USB. The program runs the scrcpy server itself
(`adb push scrcpy-server`, start it with `video=false control=false audio_source=mic-unprocessed
audio_codec=raw`, `adb forward`, read the audio socket: raw 48 kHz stereo s16le packets with scrcpy's
12-byte headers), following scrcpy's `doc/develop.md` for the protocol of the bundled scrcpy version.
`adb.exe` and `scrcpy-server` ship in the zip. If no phone is plugged in, Micro 2 lists the ordinary
input devices instead. The phone needs USB debugging on (Yassine's Redmi already has it).

## Architecture

Cargo workspace, two crates:

- `moteur` (processing): channel strip, the three blocks, EQ, mixer, meters, generators. Pure `f32`
  processing on blocks, no allocation inside `process`, no threads, no OS or GUI code, no egui/cpal
  dependency. This is the part that later goes to the board.
- `table-de-mixage` (app): egui/eframe window, cpal streams, file decoding (symphonia), Piper process,
  scrcpy/adb client, WAV writer (hound), resampling (rubato) for devices or files not at 48 kHz.

Threads: one cpal output stream at 48 kHz drives the mix. Each live input has its own cpal input
stream (or the scrcpy socket reader) that pushes into a lock-free ring buffer (`ringbuf`); the output
callback pulls a block from every source, runs each channel's strip, mixes, runs the master. Controls go
from the UI to the audio thread through atomics or a lock-free command queue; meters and plot data come
back through a ring buffer the UI reads at about 30 frames per second. The audio callback never locks a
mutex shared with the UI.

## Packaging

`cargo xwin build --release --target x86_64-pc-windows-msvc`, then a zip:

```
Table de mixage/
  table-de-mixage.exe
  piper/            piper.exe, its DLLs, espeak-ng-data
  voix/             fr_FR-siwis-medium.onnx + .onnx.json
  telephone/        adb.exe, AdbWinApi.dll, AdbWinUsbApi.dll, scrcpy-server
  LISEZMOI.txt      how to start, how to plug the phone, the demo steps
```

Missing optional parts degrade gracefully: no Piper means the Voix channel says so in its box; no adb
means Micro 2 only lists normal devices. Linux build for the asus with the same layout (system `adb`,
Python piper allowed).

## Testing

- Unit tests in `moteur` on synthetic signals:
  Limiteur: a 0 dBFS burst comes out with peak ≤ S + 0.5 dB, a -30 dBFS sine passes unchanged.
  Coupe-cri: a -3 dBFS burst of 200 ms inside a -25 dBFS sine is muted (output RMS during the burst at
  least 30 dB lower), the sine before and after passes within 1 dB.
  Anti-bruit: white noise at -40 dBFS plus a 1 kHz tone at -12 dBFS; after learning the noise, noise
  power drops by at least 15 dB and the tone stays within 2 dB.
  EQ: with Graves at +12 dB and others at 0, gain at 100 Hz is about +12 dB and at 1 kHz about 0 dB.
- Manual: run on the asus with wired earbuds; run the Windows build on hp-win; check latency figure in
  the status text; rehearse the demo script below.

## Demo script (for LISEZMOI and rehearsal)

1. Start, Micro 1 on earbuds, listen on earbuds. Talk: clean voice, plot moves.
2. Scream with only Limiteur ticked: comes out at talking level; Signal view shows the flattened peak.
3. Tick Coupe-cri: scream does not come out at all, talking still does.
4. Unmute Bruit (50 Hz or souffle) and/or turn on a fan; tick Anti-bruit on that channel and Micro 1,
   press "Apprendre le bruit": the noise disappears, voice stays. Spectrum view shows the drop.
5. Play a music file under the voice; type a sentence into Voix and press Dire.
6. Plug the phone, pick it as Micro 2, repeat 2 and 3 with the phone.

## Schedule

- Thu 26: workspace, cpal duplex passthrough with meters on Linux, Limiteur + tests, UI skeleton in the
  plain look.
- Fri 27: Coupe-cri, Anti-bruit + tests, plot (signal and spectrum).
- Sat 28: Musique, Bruit, Voix (Piper), EQ, Enregistrer.
- Sun 29: phone via scrcpy; first Windows build on hp-win.
- Mon 30: fixes, zip, LISEZMOI, rehearsal on Windows. Freeze in the evening.

## Left to the builder

- Exact crate versions; egui widget code for group boxes and vertical sliders.
- Whether parameters sit in a small dialog or inline.
- Resampler settings, ring buffer sizes, how the measured latency is computed.
- Piper Windows release to bundle (the standalone C++ `piper_windows_amd64.zip`) and where to get
  `adb` for Windows (platform-tools).
- Linux packaging details.
