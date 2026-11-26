# Table de mixage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows (and Linux) mixing console, "Table de mixage", with 5 live channels and switchable Limiteur, Coupe-cri and Anti-bruit blocks, ready for the PEI121 demo on Tuesday 2026-12-01.

**Architecture:** A Cargo workspace with two crates. `moteur` is pure block processing on `f32` (blocks, channel strip, mixer) with no OS, thread or GUI code, so it can later go to a board. `table-de-mixage` is the eframe/egui app: cpal streams, sources (files, Piper, phone), WAV recording, the plain Windows-tool UI and the plot. UI and audio thread share settings through a triple buffer and get meters and plot samples back through a triple buffer and SPSC rings; the audio callback never blocks.

**Tech Stack:** Rust 1.96, eframe 0.37 + egui_plot (the release built on the same egui as eframe), cpal 0.18, ringbuf 0.6, triple_buffer 9, realfft 3.5, rubato 5, symphonia 0.6 (mp3, wav, flac features), hound 3.5, directories 6. Windows build with `cargo xwin` for `x86_64-pc-windows-msvc`.

**Spec:** `docs/superpowers/specs/2026-11-26-table-de-mixage-design.md`

## Global Constraints

- Names: window title `Table de mixage`, binary `table-de-mixage`, crates `moteur` and `table-de-mixage`, repo `~/Projects/school/table-de-mixage`.
- `moteur::FE = 48_000.0` Hz, `moteur::BLOC = 256` samples. All processing at 48 kHz.
- `moteur` depends on `realfft` only. Every `traiter` takes a `&mut [f32]` of length `BLOC`, allocates nothing and never panics on NaN or inf input; buffers are allocated in `new()`.
- The cpal output callback never locks a mutex, never allocates, never does I/O. It talks to the rest only through triple buffers, SPSC rings and atomics.
- UI text in French. Plain look from the spec: background `#f0f0f0`, group box borders `#c8c8c8`, plot grid `#e6e6e6`, entrée `#1f77b4`, sortie `#ff7f0e`. Font Segoe UI on Windows (`C:\Windows\Fonts\segoeui.ttf`), bundled DejaVu Sans elsewhere, 11 px body.
- On-screen text is short labels, never sentences with colons, parentheses or parameter dumps; no em dashes in UI text, docs or commit messages.
- Runs from an unzipped folder on Windows 10/11 x64, no admin rights, no internet. Optional parts (Piper, adb) missing means a message in the channel box, never a crash.
- Commit after every task; `.superpowers/` and `target/` in `.gitignore`. Commit messages carry no attribution lines.

## Review Focus

1. Earbuds unplugged mid-demo (input or output device disappears): the app keeps running, the channel box shows "Périphérique perdu", the rest keeps mixing. Test in Task 7.
2. Input device not at 48 kHz (44.1 kHz USB or Bluetooth mics): resampled so a 1 kHz tone stays at 1 kHz. Test in Task 7.
3. Two devices with drifting clocks (Micro 1 and the output, or the phone): rings never grow the delay past 2 blocks of slack and underruns play silence. Test in Task 7.
4. A clipped scream, NaN or inf reaching the mixer: output stays finite and never exceeds 0 dBFS with the master Limiteur on. Test in Task 6.
5. Piper missing, a phone not authorised for USB debugging, an unreadable music file: an error string in the channel box within 5 s, no hang. Tests in Tasks 10 and 12.

---

## File Structure

```
Cargo.toml                         workspace: members moteur, app
.gitignore
moteur/Cargo.toml
moteur/src/lib.rs                  FE, BLOC, pub mod ...
moteur/src/niveau.rs               dB conversions, rms, crete
moteur/src/limiteur.rs             Limiteur
moteur/src/coupe_cri.rs            CoupeCri
moteur/src/anti_bruit.rs           AntiBruit (STFT spectral subtraction + gate)
moteur/src/egaliseur.rs            TP filters (passe_bas, passe_haut, passe_bande) + Egaliseur
moteur/src/generateur.rs           TypeBruit, Generateur
moteur/src/voie.rs                 ReglagesVoie, Voie (channel strip)
moteur/src/table.rs                ReglagesTable, EntreesBloc, Table (mix + master)
app/Cargo.toml                     package name table-de-mixage
app/assets/DejaVuSans.ttf
app/src/main.rs                    eframe start, fonts, App struct
app/src/style.rs                   plain Windows visuals and widgets
app/src/anneau.rs                  ring helpers + Reechantillonneur
app/src/audio.rs                   devices, streams, Audio::demarrer, LiensUi, Retour
app/src/ui.rs                      menu, channel boxes, Sortie box, status text
app/src/trace.rs                   spectre_db, Trace (signal and spectrum views)
app/src/sources.rs                 decoder_fichier, LecteurMusique, trouver_piper, piper_dire, Voix thread
app/src/enregistreur.rs            dossier_enregistrements, Enregistreur
app/src/telephone.rs               scrcpy client (adb + raw audio socket)
packaging/windows.sh               build + assemble + zip
packaging/LISEZMOI.txt
media/README.md                    screenshot index (project-media skill)
```

Tests live in `#[cfg(test)] mod tests` at the bottom of each file. Shared test helpers for `moteur` (sine, white noise, RMS over a range, gain at a frequency by Goertzel) go in `moteur/src/lib.rs` under `#[cfg(test)] pub(crate) mod essais`.

---

### Task 1: Workspace, levels, repo

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `moteur/Cargo.toml`, `moteur/src/lib.rs`, `moteur/src/niveau.rs`, `app/Cargo.toml`, `app/src/main.rs` (prints nothing yet, opens an empty eframe window titled `Table de mixage`)

**Interfaces:**
- Produces: `moteur::{FE, BLOC}`; `moteur::niveau::{db_vers_lin(db: f32) -> f32, lin_vers_db(x: f32) -> f32, rms(x: &[f32]) -> f32, crete(x: &[f32]) -> f32}`; test helpers `moteur::essais::{sinus(f: f32, amp: f32, n: usize) -> Vec<f32>, bruit_blanc(rms: f32, n: usize, graine: u32) -> Vec<f32>, rms_db(x: &[f32]) -> f32, gain_db_a(x_in: &[f32], x_out: &[f32], f: f32) -> f32}`.

- [ ] **Step 1:** `git init` in `~/Projects/school/table-de-mixage`, write `.gitignore` (`target/`, `.superpowers/`), workspace `Cargo.toml` with `members = ["moteur", "app"]`, `resolver = "2"`, release profile `opt-level = 3`, `lto = "thin"`.
- [ ] **Step 2: Write the failing tests** in `niveau.rs`

```rust
#[test] fn zero_db_vaut_un() { assert!((db_vers_lin(0.0) - 1.0).abs() < 1e-6); }
#[test] fn moins_six_db_vaut_un_demi() { assert!((db_vers_lin(-6.0206) - 0.5).abs() < 1e-4); }
#[test] fn moins_infini_vaut_zero() { assert_eq!(db_vers_lin(f32::NEG_INFINITY), 0.0); assert_eq!(lin_vers_db(0.0), f32::NEG_INFINITY); }
#[test] fn rms_sinus_pleine_echelle() { let x = essais::sinus(1000.0, 1.0, 4800); assert!((lin_vers_db(rms(&x)) + 3.01).abs() < 0.05); }
```

- [ ] **Step 3:** Run `cargo test -p moteur niveau`. Expected: FAIL, functions not defined.
- [ ] **Step 4:** Implement the four functions and the `essais` helpers (white noise from a xorshift32, no `rand` crate; `gain_db_a` by Goertzel on the last half of each signal).
- [ ] **Step 5:** Run `cargo test -p moteur niveau` (PASS) and `cargo run -p table-de-mixage` (an empty window titled Table de mixage opens).
- [ ] **Step 6:** Commit `moteur: niveaux en dB, espace de travail`.

### Task 2: Limiteur

**Files:** Create `moteur/src/limiteur.rs`

**Interfaces:**
- Produces: `Limiteur::new(seuil_db: f32) -> Limiteur`, `set_seuil_db(&mut self, s: f32)`, `traiter(&mut self, x: &mut [f32])`, `retard(&self) -> usize` (96 samples, the 2 ms look-ahead).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn rafale_pleine_echelle_limitee_au_seuil() {
    // 200 ms of 1 kHz at 0 dBFS after 100 ms of silence
    let y = traiter_tout(Limiteur::new(-12.0), &rafale);
    assert!(lin_vers_db(crete(&y)) <= -12.0 + 0.5);
}
#[test] fn signal_faible_inchange() {
    // -30 dBFS sine: output equals input delayed by retard() within 1e-4
}
#[test] fn sortie_finie_sur_nan() { /* a NaN in the block gives finite output */ }
```

- [ ] **Step 2:** Run `cargo test -p moteur limiteur`. Expected: FAIL.
- [ ] **Step 3:** Implement. Delay line of 96 samples; envelope on the undelayed |x| with attack 1 ms and release 150 ms (one-pole coefficients `exp(-1/(t·FE))`); `g = min(1, S/env)` applied to the delayed sample; final safety clamp of |y| to S. Non-finite input samples are replaced by 0 before anything else.
- [ ] **Step 4:** Run `cargo test -p moteur limiteur`. Expected: PASS.
- [ ] **Step 5:** Commit `moteur: limiteur`.

### Task 3: Coupe-cri

**Files:** Create `moteur/src/coupe_cri.rs`

**Interfaces:**
- Produces: `CoupeCri::new(seuil_db: f32) -> CoupeCri`, `set_seuil_db(&mut self, t: f32)`, `traiter(&mut self, x: &mut [f32])`, `retard(&self) -> usize` (960 samples, 20 ms), `coupe(&self) -> bool` (true while muted, for the UI).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn cri_coupe_entierement() {
    // 1 kHz at -25 dBFS for 1 s with a 200 ms burst at -3 dBFS starting at 400 ms
    // compare output (shifted by retard()) with input over the burst window
    assert!(rms_db(&y_burst) <= rms_db(&x_burst) - 30.0);
}
#[test] fn parole_avant_et_apres_passe() {
    // same signal: the 300 ms before the burst, and from 750 ms (after hold + 50 ms fade-in) to the end,
    // pass within 1 dB
}
#[test] fn parole_sous_le_seuil_jamais_coupee() { /* -20 dBFS sine for 2 s: coupe() never true */ }
```

- [ ] **Step 2:** Run `cargo test -p moteur coupe_cri`. Expected: FAIL.
- [ ] **Step 3:** Implement with the spec's values: 960-sample delay line, running RMS over 480 samples on the undelayed input, trigger after 240 consecutive samples above T, gain ramp to 0 in 240 samples, hold 14 400 samples after RMS falls below T, ramp back to 1 in 2 400 samples. Gain applied to the delayed signal.
- [ ] **Step 4:** Run `cargo test -p moteur coupe_cri`. Expected: PASS.
- [ ] **Step 5:** Commit `moteur: coupe-cri avec anticipation de 20 ms`.

### Task 4: Anti-bruit

**Files:** Create `moteur/src/anti_bruit.rs`

**Interfaces:**
- Produces: `AntiBruit::new() -> AntiBruit`, `apprendre(&mut self)` (the next 1 s of input becomes the noise profile), `apprend(&self) -> bool`, `traiter(&mut self, x: &mut [f32])`, `retard(&self) -> usize` (the STFT delay in samples, measured in a test with an impulse, at most 512).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn bruit_blanc_baisse_de_15_db() {
    // apprendre(), feed 1 s of white noise at -40 dBFS, then 2 s more of the same noise
    // output RMS over the last second <= input RMS - 15 dB
}
#[test] fn ton_garde_dans_le_bruit() {
    // after learning on noise alone, feed noise + 1 kHz at -12 dBFS for 2 s
    // gain_db_a(input, output, 1000.0) within ±2 dB
}
#[test] fn silence_reste_silence() { /* -70 dBFS noise: output RMS <= -95 dBFS after the gate settles */ }
```

- [ ] **Step 2:** Run `cargo test -p moteur anti_bruit`. Expected: FAIL.
- [ ] **Step 3:** Implement: STFT with frame 512, hop 256 (= BLOC), periodic Hann analysis and synthesis windows normalised for unity overlap-add, `realfft` plans and scratch made in `new()`. Per bin `|Y| = max(|X| - 2·N, 0.05·|X|)`, phase of X kept. Learning averages |X| over 187 frames. Automatic update: keep the minimum frame energy over the last 2 s; a frame with energy below twice that minimum updates `N = 0.95·N + 0.05·|X|`. Then the gate: RMS per block, below -50 dBFS attenuate 30 dB with 5 ms attack and 100 ms release.
- [ ] **Step 4:** Run `cargo test -p moteur anti_bruit`. Expected: PASS.
- [ ] **Step 5:** Commit `moteur: anti-bruit par soustraction spectrale et porte`.

### Task 5: Course filters and EQ

**Files:** Create `moteur/src/egaliseur.rs`

**Interfaces:**
- Produces: `passe_bas(fc: f32, m: usize) -> Vec<f32>`, `passe_haut(fc: f32, m: usize) -> Vec<f32>`, `passe_bande(f_bas: f32, f_haut: f32, m: usize) -> Vec<f32>` (coefficients `T_e·h_k`, Hamming window, `m` odd, same formulas as `coefficients_du_filtre` in the TP `simulation.py`: low-pass `2fc·sinc(2fc(t - t0))`, high-pass `δ - low-pass`, band-pass `low-pass(f_haut) - low-pass(f_bas)`); `Egaliseur::new() -> Egaliseur` (M = 511, cuts at 250 Hz and 4 kHz), `set_gains_db(&mut self, graves: f32, mediums: f32, aigus: f32)` (no allocation), `traiter(&mut self, x: &mut [f32])`, `retard(&self) -> usize` (255 when active, 0 when all gains are 0 dB).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn passe_bas_gain_continu_un() { assert!((passe_bas(250.0, 511).iter().sum::<f32>() - 1.0).abs() < 0.01); }
#[test] fn passe_haut_gain_continu_nul() { assert!(passe_haut(4000.0, 511).iter().sum::<f32>().abs() < 0.01); }
#[test] fn graves_plus_douze() {
    // set_gains_db(12, 0, 0): gain at 100 Hz = 12 ± 1 dB, at 1 kHz = 0 ± 1 dB
}
#[test] fn plat_ne_touche_rien() { /* gains 0,0,0: output == input exactly, retard() == 0 */ }
```

- [ ] **Step 2:** Run `cargo test -p moteur egaliseur`. Expected: FAIL.
- [ ] **Step 3:** Implement: `h = g_G·passe_bas(250) + g_M·passe_bande(250, 4000) + g_A·passe_haut(4000)` into a preallocated kernel, FFT of size 1024 (≥ 256 + 511 - 1), overlap-add per block. `set_gains_db` recomputes the kernel spectrum in place from three spectra cached in `new()`.
- [ ] **Step 4:** Run `cargo test -p moteur egaliseur`. Expected: PASS.
- [ ] **Step 5:** Commit `moteur: égaliseur 3 bandes avec les filtres du TP`.

### Task 6: Noise generator, channel strip, mixer

**Files:** Create `moteur/src/generateur.rs`, `moteur/src/voie.rs`, `moteur/src/table.rs`

**Interfaces:**
- Consumes: Tasks 1 to 5.
- Produces:
  - `enum TypeBruit { Blanc, Ronflement50, Souffle }` (Copy); `Generateur::new(graine: u32)`, `set_type(&mut self, t: TypeBruit)`, `remplir(&mut self, x: &mut [f32])`, all at -20 dBFS RMS. Ronflement50 = 50 Hz with harmonics 100, 150, 200 Hz at -6, -12, -18 dB relative. Souffle = pink noise (Paul Kellet filter on white).
  - `#[derive(Clone, Copy)] struct ReglagesVoie { volume_db: f32, muet: bool, anti_bruit: bool, coupe_cri: bool, limiteur: bool, graves_db: f32, mediums_db: f32, aigus_db: f32, seuil_limiteur_db: f32, seuil_cri_db: f32, apprendre: u32 }` with `Default` = 0 dB, all off, EQ flat, -12, -10, 0. `apprendre` is a counter: when it changes, the strip calls `AntiBruit::apprendre()`.
  - `Voie::new()`, `traiter(&mut self, r: &ReglagesVoie, x: &mut [f32], sonde: &mut [f32])`: chain Anti-bruit → Coupe-cri → Limiteur → EQ (each only when ticked), copies the result into `sonde` (before volume), then volume and mute in place. `crete(&self) -> f32` (peak after volume), `retard(&self) -> usize` (sum of active blocks), `coupe(&self) -> bool`.
  - `const NB_VOIES: usize = 5` (0 Micro 1, 1 Micro 2, 2 Musique, 3 Voix, 4 Bruit). `#[derive(Clone, Copy)] struct ReglagesTable { voies: [ReglagesVoie; 5], master_db: f32, master_limiteur: bool, type_bruit: TypeBruit, voie_sondee: usize }`, default master 0 dB, master Limiteur on at -1 dBFS.
  - `struct EntreesBloc<'a> { micro1: &'a [f32], micro2: &'a [f32], musique_g: &'a [f32], musique_d: &'a [f32], voix: &'a [f32] }`.
  - `Table::new()`, `traiter(&mut self, r: &ReglagesTable, e: &EntreesBloc, sortie_g: &mut [f32], sortie_d: &mut [f32])`, `cretes(&self) -> [f32; 5]`, `cretes_master(&self) -> [f32; 2]`, `sonde(&self) -> (&[f32], &[f32])` (entrée and sortie of `voie_sondee` for the last block, Musique uses left), `retard(&self, voie: usize) -> usize`, `coupe(&self, voie: usize) -> bool`. Musique runs two `Voie`s (left, right) with the same settings; mono voices go equally to both sides.

- [ ] **Step 1: Write the failing tests** in `table.rs` (and one in `generateur.rs`)

```rust
#[test] fn bruit_a_moins_vingt_db() { /* each TypeBruit: rms_db over 1 s within ±1 dB of -20 */ }
#[test] fn muet_donne_zero() { /* Micro 1 muted: sortie all 0.0 */ }
#[test] fn volume_moins_six() { /* Micro 1 at -6.02 dB, others muted: sortie peak = half input peak ±1 % */ }
#[test] fn master_jamais_au_dessus_de_zero_dbfs() {
    // all 5 voices at +6 dB fed 0 dBFS sines: crete(sortie) <= 1.0 with master_limiteur on
}
#[test] fn nan_et_inf_donnent_sortie_finie() { /* NaN in micro1, inf in voix: every sortie sample finite and <= 1.0 */ }
#[test] fn sonde_avant_volume() { /* Micro 1 at -20 dB: sonde().1 equals the unattenuated strip output */ }
```

- [ ] **Step 2:** Run `cargo test -p moteur`. Expected: the new tests FAIL.
- [ ] **Step 3:** Implement the three files.
- [ ] **Step 4:** Run `cargo test -p moteur`. Expected: all PASS.
- [ ] **Step 5:** Commit `moteur: voies, générateur de bruit, table`.

### Task 7: Live audio on Linux (Micro 1 to output)

**Files:** Create `app/src/anneau.rs`, `app/src/audio.rs`; modify `app/src/main.rs`

**Interfaces:**
- Consumes: `moteur::table::*`.
- Produces:
  - `anneau::tirer_bloc(cons: &mut HeapCons<f32>, dest: &mut [f32]) -> bool` (fills `dest`, zero-fills on underrun, returns false on underrun); `anneau::borner(cons: &mut HeapCons<f32>, max: usize)` (drops oldest samples beyond `max`, used with `max = 2 * BLOC` per pull); `anneau::Reechantillonneur::new(fe_in: u32, canaux: usize)`, `pousser(&mut self, entrelace: &[f32], prod: &mut HeapProd<f32>)` (mono-mixes or keeps stereo, converts to 48 kHz with rubato, buffers preallocated).
  - `audio::lister_entrees() -> Vec<String>`, `audio::lister_sorties() -> Vec<String>`.
  - `enum SourceMicro { Aucun, Peripherique(String), Telephone }`; `struct ChoixAudio { sortie: Option<String>, micro1: SourceMicro, micro2: SourceMicro }`.
  - `#[derive(Clone, Copy, Default)] struct Retour { cretes: [f32; 5], master: [f32; 2], coupe: [bool; 5], latence_ms: f32, perdu: [bool; 2] /* micro1, micro2 */, sortie_perdue: bool }`.
  - `struct LiensUi { reglages: triple_buffer::Input<ReglagesTable>, retour: triple_buffer::Output<Retour>, sonde_entree: HeapCons<f32>, sonde_sortie: HeapCons<f32>, musique: HeapProd<f32> /* interleaved stereo */, voix: HeapProd<f32>, enregistrement: HeapCons<f32> /* interleaved stereo */, enregistrer: Arc<AtomicBool> }`.
  - `Audio::demarrer(choix: &ChoixAudio) -> Result<(Audio, LiensUi), String>`; dropping `Audio` stops every stream. A device change in the UI drops the old `Audio` and calls `demarrer` again.

- [ ] **Step 1: Write the failing tests** in `anneau.rs`

```rust
#[test] fn sous_alimentation_donne_silence() { /* ring holding 100 samples: tirer_bloc returns false, dest[100..] all 0 */ }
#[test] fn derive_ne_fait_pas_grandir_le_retard() {
    // push 1000 blocks of 257 samples, pull 1000 blocks of 256 with borner(2*BLOC): occupied_len() <= 2*BLOC at the end
}
#[test] fn reechantillonnage_garde_la_frequence() {
    // 1 s of 1 kHz at 44 100 Hz through Reechantillonneur: about 48 000 samples out (±1 %), strongest DFT bin at 1 kHz ± 50 Hz
}
```

- [ ] **Step 2:** Run `cargo test -p table-de-mixage anneau`. Expected: FAIL.
- [ ] **Step 3:** Implement `anneau.rs`.
- [ ] **Step 4:** Run `cargo test -p table-de-mixage anneau`. Expected: PASS.
- [ ] **Step 5:** Implement `audio.rs`. Output stream at 48 kHz stereo f32 with a fixed buffer of 256 frames when the device allows it; its callback pulls one block from each ring (with `borner`), generates Bruit, runs `Table::traiter`, writes the output, pushes the sonde samples and (when `enregistrer` is set) the output into their rings, and publishes `Retour`. Input streams ask for 48 kHz and fall back to the device default through `Reechantillonneur`. Stream error callbacks set the matching `perdu` flag in an `AtomicBool` the output callback copies into `Retour`. `latence_ms` = (output buffer + current Micro 1 ring fill + `Table::retard(0)`) / 48. Test hook: `demarrer` with an unknown device name returns `Err("Périphérique introuvable: <nom>")`, covered by a unit test.
- [ ] **Step 6:** In `main.rs`, start `Audio` with the default devices and Micro 1 on the default input, write default `ReglagesTable` each frame.
- [ ] **Step 7:** Manual check on the asus with wired earbuds: `cargo run --release`, talk, hear yourself with no crackle for 2 minutes; pull the earbuds out and back: no crash. Note the measured latency.
- [ ] **Step 8:** Commit `app: audio en direct, micro 1 vers la sortie`.

### Task 8: The window in the plain Windows look

**Files:** Create `app/src/style.rs`, `app/src/ui.rs`, `app/assets/DejaVuSans.ttf`; modify `app/src/main.rs`

**Interfaces:**
- Consumes: `LiensUi`, `Retour`, `ReglagesTable`, `audio::lister_*`.
- Produces: `style::appliquer(ctx: &egui::Context)`; `style::boite_groupe(ui: &mut egui::Ui, titre: &str, choisie: bool, contenu: impl FnOnce(&mut egui::Ui)) -> egui::Response` (titled frame, title cut into the top border, darker border when `choisie`); `style::curseur_db(ui, valeur: &mut f32) -> egui::Response` (vertical, -60 to +6 dB, -60 shown as -∞, ticks +6 0 -20 -40 -∞); `style::vumetre(ui, crete: f32)`; `ui::dessiner(ctx, etat: &mut EtatUi)` where `EtatUi` holds the `ReglagesTable` copy, `ChoixAudio`, `LiensUi`, `Audio`, the text of the Voix field, the music file name and error strings per channel.

- [ ] **Step 1:** Implement `style.rs` with the colours and font rule from Global Constraints (egui light visuals, window fill `#f0f0f0`, no rounding beyond 2 px, no shadows).
- [ ] **Step 2:** Implement `ui.rs` following the spec's Look sketch: menu bar (Fichier: Ouvrir une musique, Dossier des enregistrements, Quitter; Audio: devices and the Réglages des blocs dialog with S, T per channel and the Apprendre le bruit buttons; Affichage: Signal, Spectre; Aide: À propos and one short line per block with its formula), status text on the right (`48 000 Hz · bloc 256 · latence N ms`), one box per channel in a row (`1. Micro 1` ... `5. Bruit`, then `Sortie`) with the source combo, `curseur_db` with `vumetre`, a `Volume` dB field, three small Graves / Médiums / Aigus sliders, check boxes Anti-bruit, Coupe-cri, Limiteur, Muet, and an error line when `Retour.perdu` or a source error is set. Clicking a box sets `voie_sondee`. Changing a device rebuilds `Audio`. The plot area is an empty white panel for now.
- [ ] **Step 3:** Manual check: `cargo run --release`, every control changes the sound as named; screenshot the window with `spectacle -b -n -a -o media/fenetre.png` and compare with the third card of `.superpowers/brainstorm/2376265-1795651105/content/style.html` (same arrangement, no dark parts, no rounded cards).
- [ ] **Step 4:** Commit `app: fenêtre et commandes`.

### Task 9: The plot

**Files:** Create `app/src/trace.rs`; modify `app/src/ui.rs`

**Interfaces:**
- Consumes: `LiensUi.sonde_entree`, `sonde_sortie`, seuils S and T of the selected channel.
- Produces: `trace::spectre_db(x: &[f32]) -> Vec<f32>` (Hann, length 4096, dB re a full-scale sine, bins 0 to 24 kHz); `Trace::new()`, `Trace::alimenter(&mut self, e: &mut HeapCons<f32>, s: &mut HeapCons<f32>)` (keeps the last 9 600 samples of each), `Trace::afficher(&mut self, ui: &mut egui::Ui, vue: Vue, seuil_limiteur_db: f32, seuil_cri_db: f32)`; `enum Vue { Signal, Spectre }`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn sinus_pleine_echelle_a_zero_db() { /* 1 kHz amp 1.0: max of spectre_db within ±0.5 dB of 0, at the bin of 1 kHz */ }
#[test] fn silence_tres_bas() { /* zeros: every bin <= -120 dB, none NaN */ }
```

- [ ] **Step 2:** Run `cargo test -p table-de-mixage trace`. Expected: FAIL.
- [ ] **Step 3:** Implement with `egui_plot`: white background, black axes, grid `#e6e6e6`, entrée `#1f77b4`, sortie `#ff7f0e`, legend `entrée` / `sortie`. Signal view: last 200 ms, x in ms, y from -1 to 1, S and T as dashed horizontal lines at ± their linear values. Spectre view: 0 to 24 kHz, -100 to 0 dB, axis label `f (Hz)`.
- [ ] **Step 4:** Run `cargo test -p table-de-mixage trace` (PASS), then manual: scream with Limiteur on, the orange trace stays under the S line.
- [ ] **Step 5:** Commit `app: tracé signal et spectre`.

### Task 10: Musique and Voix sources

**Files:** Create `app/src/sources.rs`; modify `app/src/ui.rs`, `app/src/audio.rs`

**Interfaces:**
- Consumes: `LiensUi.musique`, `LiensUi.voix`, `anneau::Reechantillonneur`.
- Produces: `decoder_fichier(chemin: &Path) -> Result<(Vec<f32>, Vec<f32>), String>` (whole file to 48 kHz stereo, mono duplicated); `LecteurMusique::new(prod: HeapProd<f32>)`, `ouvrir(&self, chemin: PathBuf)`, `lecture(&self, oui: bool)`, `boucle(&self, oui: bool)`, `erreur(&self) -> Option<String>` (a thread fed by an mpsc channel, pushing interleaved stereo at real-time pace); `trouver_piper() -> Option<(PathBuf, PathBuf)>` (binary and `.onnx` voice: first `<exe dir>/piper/piper[.exe]` and `<exe dir>/voix/*.onnx`, then on Linux `piper` on PATH and `~/.local/share/piper-voices/fr_FR-siwis-medium.onnx`); `piper_dire(texte: &str, piper: &Path, voix: &Path) -> Result<Vec<f32>, String>` (runs `piper --model <voix> --output_raw` with the text on stdin, 16-bit mono at the `audio.sample_rate` of the voice's `.onnx.json`, resampled to 48 kHz, killed after 15 s); `Voix::new(prod: HeapProd<f32>)`, `dire(&self, texte: String)`, `erreur(&self) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn wav_44100_vers_48000() { /* write 1 s 1 kHz mono WAV at 44 100 Hz with hound into a temp dir: 48 000 ± 1 % samples per side, peak bin at 1 kHz */ }
#[test] fn fichier_illisible_donne_erreur() { /* a text file renamed .mp3: Err(..) */ }
#[test] fn piper_absent_donne_erreur() { /* piper_dire with /nonexistent/piper: Err within 1 s */ }
#[test] fn aucun_piper_dans_dossier_vide() { /* trouver_piper with exe dir set to an empty temp dir and PATH cleared returns None */ }
```

- [ ] **Step 2:** Run `cargo test -p table-de-mixage sources`. Expected: FAIL.
- [ ] **Step 3:** Implement. For the PATH-free test, `trouver_piper` takes its search roots from a private `trouver_piper_dans(dossier_exe: &Path, chemin: Option<&OsStr>, maison: Option<&Path>)` that the public function calls.
- [ ] **Step 4:** Run `cargo test -p table-de-mixage sources`. Expected: PASS.
- [ ] **Step 5:** `Audio::demarrer` now builds `LecteurMusique` and `Voix` from the two producers and `LiensUi` carries them as `musique: LecteurMusique` and `voix: Voix` instead of the raw producers, so a device change rebuilds them (the UI re-opens the current music file). Download `fr_FR-siwis-medium.onnx` and `.onnx.json` from the `rhasspy/piper-voices` Hugging Face repo into `~/.local/share/piper-voices/`. Wire the UI: Musique box gets Ouvrir, lecture/pause and Boucle; Voix box a text field and a `Dire` button; Bruit box a combo Bruit blanc / Ronflement 50 Hz / Souffle feeding `type_bruit`. Errors show in the box.
- [ ] **Step 6:** Manual: play an MP3 under your voice; type `Bonjour monsieur` and press Dire; switch the Bruit type.
- [ ] **Step 7:** Commit `app: musique, voix de synthèse, bruit de test`.

### Task 11: Recording (dropped, then brought back 2026-11-26)

Done differently from the steps below, at the user's request: the « Enregistrer » button in the menu bar records the channel chosen for the plot, before its blocks and after them (before its volume), into `<nom>-avant.wav` and `<nom>-apres.wav` (48 kHz mono 16 bit), written as it goes by a thread (`app/src/enregistrement.rs`). Tests: `enregistrement_avant_et_apres`, and `enregistrement_vrai_du_telephone` (ignored, real phone).

**Files:** Create `app/src/enregistreur.rs`; modify `app/src/ui.rs`

**Interfaces:**
- Consumes: `LiensUi.enregistrement`, `LiensUi.enregistrer`.
- Produces: `dossier_enregistrements() -> PathBuf` (`<Documents>/Table de mixage`, from `directories::UserDirs`); `Enregistreur::new(cons: HeapCons<f32>, drapeau: Arc<AtomicBool>, dossier: PathBuf) -> Enregistreur` (a thread: when the flag turns on it opens `enregistrement-AAAAMMJJ-HHMMSS.wav`, 16-bit 48 kHz stereo, and writes what it pops; when the flag turns off it finalises the file); `dernier_fichier(&self) -> Option<PathBuf>`, `erreur(&self) -> Option<String>`. Built inside `Audio::demarrer`; `LiensUi` carries `enregistreur: Enregistreur` instead of the raw consumer.

- [ ] **Step 1: Write the failing test**

```rust
#[test] fn demi_seconde_stereo() { /* flag on, push 24 000 stereo frames, flag off, wait for dernier_fichier(): reads back with hound as 2 channels, 48 000 Hz, 24 000 frames */ }
```

- [ ] **Step 2:** Run `cargo test -p table-de-mixage enregistreur`. Expected: FAIL.
- [ ] **Step 3:** Implement; the Enregistrer check box in Sortie sets the flag; Fichier > Dossier des enregistrements opens the folder (`explorer` on Windows, `xdg-open` on Linux).
- [ ] **Step 4:** Run `cargo test -p table-de-mixage enregistreur`. Expected: PASS.
- [ ] **Step 5:** Commit `app: enregistrement de la sortie`.

### Task 12: Phone microphone (the demo mic)

Update 2026-11-26: the user wants the phone as an app. `telephone/` is the Android app « Micro table » (Rust AAudio core + Kotlin screen, `telephone/construire.sh`); the mixer's « Téléphone (Wi-Fi) » source connects to it (port 5005). The scrcpy USB source below stays as a fallback.

Decided 2026-11-26: the Redmi over USB is the microphone for the demo, so this task is required and live (same delay bound as any mic). Built after Task 11, on the day in the schedule.

**Files:** Create `app/src/telephone.rs`; modify `app/src/audio.rs`, `app/src/ui.rs`

**Interfaces:**
- Consumes: `anneau::Reechantillonneur`, the Micro 1 or Micro 2 ring producer inside `Audio::demarrer` (whichever channel has Téléphone selected).
- Produces: `trouver_adb() -> Option<PathBuf>` (`<exe dir>/telephone/adb[.exe]`, then PATH); `trouver_serveur() -> Option<PathBuf>` (`<exe dir>/telephone/scrcpy-server`, then `/usr/share/scrcpy/scrcpy-server`); `Telephone::demarrer(adb: &Path, serveur: &Path, prod: HeapProd<f32>) -> Result<Telephone, String>` (returns within 5 s, error text in French for: no device, device unauthorised, Android below 11); dropping it stops the server and removes the forward.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn adb_absent() { /* Telephone::demarrer with /nonexistent/adb: Err within 1 s */ }
#[test] fn fin_de_flux_marque_perdu() { /* a local TcpListener writes 480 s16le stereo frames then closes: the reader pushes 480 samples into the ring and sets the lost flag within 1 s */ }
#[test] fn paquet_brut_vers_mono() { /* decode a byte buffer of s16le stereo frames with fn pcm_vers_mono(&[u8]) -> Vec<f32>: [(16384, -16384), (32767, 32767)] -> [0.0, ~1.0] */ }
```

- [ ] **Step 2:** Run `cargo test -p table-de-mixage telephone`. Expected: FAIL.
- [ ] **Step 3:** Implement following `doc/develop.md` of scrcpy 5.0.1 (check the option names against that version): `adb push <serveur> /data/local/tmp/scrcpy-server.jar`; `adb forward tcp:<port> localabstract:scrcpy_<scid>` with a random 31-bit hex `scid`; start `adb shell CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 5.0.1 scid=<scid> tunnel_forward=true video=false control=false audio=true audio_source=mic-unprocessed audio_codec=raw raw_stream=true`; connect the TCP socket with retries for up to 5 s; read s16le stereo 48 kHz, `pcm_vers_mono`, push into that channel's ring. The ring goes through the same `borner` (packet size + 2 blocks) as the other mics, so the phone cannot pile up delay, and the status latency includes it. End of stream or a socket error sets the channel's `perdu` flag (`Téléphone perdu` in the box); picking Téléphone again reconnects. `SourceMicro::Telephone` appears in the Micro 1 and Micro 2 combos when `trouver_adb()` and `trouver_serveur()` succeed (picked by hand; no automatic switch at start, dropped 2026-11-26). The device is the first authorised one at Android 11 or above. `adb forward tcp:0` picks the port. The server deletes its jar on start, so it is pushed every time.
- [ ] **Step 4:** Run `cargo test -p table-de-mixage telephone`. Expected: PASS.
- [ ] **Step 5:** Manual on the hp with the Redmi on USB (`adb -s 3ad2f4880013` is the Redmi; pass `-s` when several devices are attached): Micro 1 = Téléphone, talk into the phone, scream with Limiteur then Coupe-cri on. Live check: Micro 2 = laptop mic, clap once next to both, watch the Signal view; the gap between the phone's click and the laptop mic's click is the phone's extra delay (target under 80 ms; note the value in the ledger). Unplug the phone: `Téléphone perdu` within 2 s, the rest keeps mixing; replug and pick Téléphone again.
- [ ] **Step 6:** Commit `app: micro du téléphone par scrcpy`.

### Task 13: Windows build, zip, rehearsal

**Files:** Create `packaging/windows.sh`, `packaging/LISEZMOI.txt`, `media/README.md`

**Interfaces:**
- Consumes: the whole app.

- [ ] **Step 1:** Load the `windows-desktop-packaging` skill (Windows over SSH, screenshots, Smart App Control). Write `packaging/windows.sh`: `cargo xwin build --release --target x86_64-pc-windows-msvc -p table-de-mixage`, then assemble `dist/Table de mixage/` with the exe, `piper/` (from the standalone `piper_windows_amd64.zip` release), `voix/` (the siwis voice), `telephone/` (`adb.exe`, `AdbWinApi.dll`, `AdbWinUsbApi.dll` from Google platform-tools for Windows, and `/usr/share/scrcpy/scrcpy-server`), `LISEZMOI.txt`, and zip it to `dist/Table-de-mixage-windows.zip`. Downloads are cached in `packaging/cache/` (git-ignored).
- [ ] **Step 2:** Write `LISEZMOI.txt` in French: how to start, how to plug the phone (USB debugging), the six demo steps from the spec.
- [ ] **Step 3:** Run `bash packaging/windows.sh`. Expected: the zip exists, under 150 MB.
- [ ] **Step 4:** Copy the zip to hp-win with laptop-link (dry-run first, then `--apply`), unzip, start it, take a screenshot over SSH. Before: authorise USB debugging for hp-win on the Redmi, and check that Windows installed an ADB driver for it (`adb devices` lists it; if not, install the Google USB driver and say so in LISEZMOI). Check: window opens, Micro 1 on the phone, Voix speaks, a scream is limited and then cut.
- [ ] **Step 5:** Run the six demo steps on Windows and on the asus. Fix what fails.
- [ ] **Step 6:** Following the `project-media` skill, put the window screenshot in `media/` (no demo recording: a phone video of the rehearsal does that job) with `media/README.md`.
- [ ] **Step 7:** Commit `packaging: zip Windows, LISEZMOI, médias`.

---

## Schedule against the deadline

| Day | Tasks |
|-----|-------|
| Thu 26 | 1 to 5 |
| Fri 27 | 6, 7, 8 |
| Sat 28 | 9, 10, 11 |
| Sun 29 | 12, first run of 13 on hp-win |
| Mon 30 | 13 finished, rehearsal, freeze in the evening |

If a day slips, drop the EQ (Task 5 and its sliders). Recording (Task 11) is already dropped. The phone (Task 12) is not dropped: it is the demo microphone.
