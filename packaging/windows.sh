#!/usr/bin/env bash
# Construit le dossier « Table de mixage » pour une clé USB, et son zip.
# Usage : bash packaging/windows.sh   (depuis Linux, avec cargo-xwin)
set -euo pipefail
cd "$(dirname "$0")/.."
CACHE=packaging/cache
SORTIE="dist/Table de mixage"
mkdir -p "$CACHE"

# le cache de xwin sur le disque NTFS casse les .lib du SDK : on le garde sur ext4
export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$HOME/.local/share/xwin-cache}"
cargo xwin build --release --target x86_64-pc-windows-msvc -p table-de-mixage

PIPER_ZIP="$CACHE/piper_windows_amd64.zip"
[ -f "$PIPER_ZIP" ] || curl -sSL -o "$PIPER_ZIP" https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip
VOIX="$HOME/.local/share/piper-voices/fr_FR-siwis-medium.onnx"
[ -f "$VOIX" ] || { echo "voix absente : $VOIX" >&2; exit 1; }

rm -rf "$SORTIE"
mkdir -p "$SORTIE/voix" "$SORTIE/telephone"
cp target/x86_64-pc-windows-msvc/release/table-de-mixage.exe "$SORTIE/"
unzip -q "$PIPER_ZIP" -d "$SORTIE"
cp "$VOIX" "$SORTIE/voix/"
# le Piper Windows de 2023 refuse les phonèmes de plusieurs caractères (diphtongues anglaises, inutiles en français)
python3 -I -c '
import json, sys
src, dst = sys.argv[1], sys.argv[2]
c = json.load(open(src, encoding="utf-8"))
c["phoneme_id_map"] = {k: v for k, v in c["phoneme_id_map"].items() if len(k) == 1}
json.dump(c, open(dst, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
' "$VOIX.json" "$SORTIE/voix/fr_FR-siwis-medium.onnx.json"

APK=telephone/app/build/outputs/apk/debug/app-debug.apk
if [ -f "$APK" ]; then cp "$APK" "$SORTIE/telephone/Micro table.apk"; else echo "attention : APK absent (telephone/construire.sh)" >&2; fi

# fins de ligne Windows pour le .bat et le texte
sed 's/$/\r/' packaging/lancer.bat > "$SORTIE/Lancer la table de mixage.bat"
sed 's/$/\r/' packaging/LISEZMOI.txt > "$SORTIE/LISEZMOI.txt"

rm -f dist/Table-de-mixage-windows.zip
(cd dist && python3 -m zipfile -c Table-de-mixage-windows.zip "Table de mixage")
du -sh "$SORTIE" dist/Table-de-mixage-windows.zip
