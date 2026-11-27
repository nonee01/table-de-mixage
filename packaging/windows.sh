#!/usr/bin/env bash
# Construit le dossier « Table de mixage » pour une clé USB, et son zip.
# Usage : bash packaging/windows.sh   (depuis Linux avec cargo-xwin, ou sous Windows dans Git Bash / GitHub Actions)
set -euo pipefail
cd "$(dirname "$0")/.."
CACHE=packaging/cache
SORTIE="dist/Table de mixage"
mkdir -p "$CACHE"

PY=$(command -v python3 || command -v python)
if [ "${OS:-}" = "Windows_NT" ]; then
  cargo build --release -p table-de-mixage
  EXE=target/release/table-de-mixage.exe
else
  # le cache de xwin sur le disque NTFS casse les .lib du SDK : on le garde sur ext4
  export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$HOME/.local/share/xwin-cache}"
  cargo xwin build --release --target x86_64-pc-windows-msvc -p table-de-mixage
  EXE=target/x86_64-pc-windows-msvc/release/table-de-mixage.exe
fi

PIPER_ZIP="$CACHE/piper_windows_amd64.zip"
[ -f "$PIPER_ZIP" ] || curl -sSL -o "$PIPER_ZIP" https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip
VOIX="$HOME/.local/share/piper-voices/fr_FR-siwis-medium.onnx"
if [ ! -f "$VOIX" ]; then
  VOIX="$CACHE/fr_FR-siwis-medium.onnx"
  HF=https://huggingface.co/rhasspy/piper-voices/resolve/main/fr/fr_FR/siwis/medium
  [ -f "$VOIX" ] || curl -sSL -o "$VOIX" "$HF/fr_FR-siwis-medium.onnx"
fi

rm -rf "$SORTIE"
mkdir -p "$SORTIE/voix" "$SORTIE/telephone"
cp "$EXE" "$SORTIE/"
unzip -q "$PIPER_ZIP" -d "$SORTIE"
cp "$VOIX" "$SORTIE/voix/"
# le Piper Windows de 2023 refuse les phonèmes de plusieurs caractères : packaging/voix en a une copie sans eux
cp packaging/voix/fr_FR-siwis-medium.onnx.json "$SORTIE/voix/"

APK=telephone/app/build/outputs/apk/debug/app-debug.apk
# dans la CI le zip publie doit avoir l'appli : son absence est une erreur
if [ -f "$APK" ]; then cp "$APK" "$SORTIE/telephone/Micro table.apk"; else echo "attention : APK absent (telephone/construire.sh)" >&2; [ -z "${CI:-}" ] || exit 1; fi

# le .bat est déjà en fins de ligne Windows (.gitattributes) ; le texte, on le convertit
cp packaging/lancer.bat "$SORTIE/Lancer la table de mixage.bat"
cp packaging/smart-app-control.bat "$SORTIE/"
sed 's/$/\r/' packaging/LISEZMOI.txt > "$SORTIE/LISEZMOI.txt"

rm -f dist/Table-de-mixage-windows.zip
(cd dist && "$PY" -m zipfile -c Table-de-mixage-windows.zip "Table de mixage")
du -sh "$SORTIE" dist/Table-de-mixage-windows.zip
