# Micro table v2: the phone app records, shows and controls

Asked by the user on 2026-11-26, answers to the questions:

- Recording can start and stop from either side (phone button or the mixer's « Enregistrer »); the files always land on both the PC and the phone.
- What to record is chosen with three boxes, on both sides: **avant** (the phone channel before its blocks), **après** (after its blocks, before its volume), **mix** (the whole output, stereo, as heard).
- Look: a modern light phone app (rounded cards, large type, one accent colour, a big round record button), not the PC's Windows look and not dark.
- The phone screen shows: the live waveform (avant blue `#1f77b4`, après orange `#ff7f0e`, the PC plot colours), a level meter, the list of recordings (tap to play), and the block switches of the phone channel (Anti-bruit, Coupe-cri, Limiteur).

The filters run on the PC, so « après » and « mix » only exist there: the PC sends traces, levels and the finished files back to the phone.

## Protocol (replaces the raw stream of the Wi-Fi link only; the USB scrcpy source stays raw)

After the 6-digit code, both directions carry frames: `type: u8`, `len: u32 le`, `payload`.

Phone to PC:
- `A` audio: s16le mono 48 kHz (mono now: no more doubled stereo).
- `R` record: `u8` 1 start / 0 stop, `u8` mask (1 avant, 2 après, 4 mix).
- `B` blocks: `u8` mask (1 anti-bruit, 2 coupe-cri, 4 limiteur) for the channel the phone feeds.

PC to phone (the PC sends nothing until the code is accepted):
- `T` traces, 30 per second: `u16` n, then n `i16` avant and n `i16` après (n = 256, decimated last 33 ms), then `u16` level after in thousandths.
- `S` state: `u8` recording, `u8` mask, `u8` blocks, `u32` seconds.
- `F` file: `u8` name length, name (`Micro 1-avant.wav`), then the WAV bytes. Sent after a stop, one frame per file, over the same socket (audio frames from the phone keep flowing; the PC sends files from a separate thread holding a clone of the socket, with a write lock shared with the `T`/`S` sender).

Limits: a frame over 64 MB is refused (bad length = close). Files over 64 MB (about 11 minutes of mono avant) are kept on the PC only, and the phone is told by an `S` with a flag (shortcut).

## Tasks

### 1. Framing in both crates
`telephone/rust`: `fn ecrire_trame(w, type, payload)`, `fn lire_trame(r) -> Result<(u8, Vec<u8>)>` with the 64 MB cap. `app/src/telephone.rs`: the same two functions (copy, 20 lines; the crates do not share code). Tests: round trip, oversize refused, truncated frame is an error.

### 2. Phone core sends `A`, reads `T`/`S`/`F`
`envoyer` writes `A` frames; a reader thread parses PC frames into shared state the Kotlin side polls through JNI: `traces() -> ShortArray`, `etat_pc() -> IntArray`, and files written to the app's cache dir then handed to Kotlin (`fichiers_recus() -> Array<String>` of paths). New JNI calls use the `jni` crate (0.21, from crates.io) since they pass arrays and strings. `enregistrer(start, mask)` and `blocs(mask)` queue `R`/`B` frames.

### 3. PC side of the protocol
`Telephone::wifi` reads frames: `A` into the ring through `pcm_vers_mono`-like mono decode; `R` and `B` go to the UI through a small `Arc<Mutex<Option<Commande>>>`. A sender thread sends `T` (from a third probe ring filled like the recording ring, only while a phone is connected) and `S` 30 times a second.

### 4. Recording with a mask and the mix
`Enregistrement::demarrer(base, cons, masque)`: avant / après mono, mix stereo; the engine pushes avant, après, mix L, mix R per sample (ring of 4 floats per frame). Boxes « avant / après / mix » next to the PC « Enregistrer » button. On stop, if a phone is connected, the chosen files are sent as `F` frames. Phone-started recordings use the phone's mask and save on the PC in `Musique/Table de mixage/` (Windows: `%USERPROFILE%\Music\Table de mixage`) without a dialog.

### 5. Blocks from the phone
`B` sets `anti_bruit`, `coupe_cri`, `limiteur` of the channel whose source is `TelephoneWifi`; the PC check boxes move with it, and `S` sends the PC's ticks back so both sides always agree.

### 6. Phone UI in Jetpack Compose
Compose (BOM 2024.12.01, cached in Gradle from sos-alert). Screens in one column: header card (address + code, state chip), waveform `Canvas` (avant blue, après orange, 30 fps from `traces()`), level bar, a big round record button with the elapsed time, three chips for avant / après / mix, three switches for the blocks, and the recordings list (MediaStore `Music/Table de mixage`, name, date, duration, play/pause with `MediaPlayer`). Received files are inserted into MediaStore so they also show in the phone's music apps. Light theme, accent `#1f77b4`.

### 7. Checks on the J7 (and the Redmi when charged)
Rust tests for every frame type. Manual: start a recording from the phone, stop from the PC, and the reverse; files appear on both sides and play; block switches tick on both sides; the waveform follows the voice. Screenshot of the phone app into `media/` (project-media).

Before Task 13 (Windows zip), so the zip carries the final phone app (`telephone/Micro table.apk` in the zip, and LISEZMOI explains the install).
