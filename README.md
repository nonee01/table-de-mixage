# Table de mixage

Une table de mixage pour le TP de traitement du signal : cinq voies (deux micros, une musique, une voix de synthèse, un bruit de test), et sur chaque voie les blocs vus en cours, que l'on coche pendant qu'on parle :

- **Limiteur** : un cri sort au niveau de la parole.
- **Coupe-cri** : un cri ne sort plus du tout, la parole passe toujours.
- **Anti-bruit** : soustraction spectrale, un ventilateur ou un ronflement de 50 Hz disparaît.
- **Égaliseur** à trois bandes (filtres RIF du cours).

Le tracé montre en direct le signal avant et après les blocs, en temps (Signal) ou en fréquence (Spectre, en dB). Le bouton **Enregistrer** écrit la voie choisie avant et après ses blocs dans deux fichiers WAV.

Un téléphone Android sert de micro par le Wi-Fi avec l'appli « Micro table » (dossier `telephone/`) : elle affiche une adresse et un code à six chiffres à taper dans la table.

## Télécharger (Windows)

Dans [Releases](https://github.com/nonee01/table-de-mixage/releases), le zip « Table-de-mixage-windows » : on le décompresse (sur une clé USB si on veut), puis on double-clique sur « Lancer la table de mixage.bat ». Rien à installer. `LISEZMOI.txt` dans le dossier explique la démonstration pas à pas.

## Construire

- Linux : `cargo run --release -p table-de-mixage`
- Windows, sans rien d'installé : double-cliquer sur `construire.bat`. Il installe les outils C++ de Microsoft et Rust s'ils manquent (environ 5 Go et 30 à 45 minutes la première fois), construit, ajoute la voix de synthèse dans `dist\Table de mixage\`, puis lance la table. Si Smart App Control est actif, il propose de le désactiver (une question, puis l'autorisation administrateur de Windows), car celui-ci bloquerait aussi un programme construit sur place.
- Windows, depuis Linux : `bash packaging/windows.sh` (cargo-xwin) ; sous Windows ou dans GitHub Actions, le même script construit en natif.
- L'appli téléphone : `telephone/construire.sh` (NDK Android, Rust, Gradle).

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

Only `table-de-mixage.exe`, built from this repository's source by the GitHub Actions workflow `.github/workflows/windows.yml`, is signed. The bundled Piper text-to-speech engine (`piper/`, MIT, with espeak-ng under GPL-3.0) and the French voice are upstream open-source components shipped unsigned.

- Committers and reviewers: [nonee01](https://github.com/nonee01)
- Approvers: [nonee01](https://github.com/nonee01)

## Privacy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

The only network use is the phone microphone: when the user types a phone's address and code, the PC connects to that phone on the local network and receives its microphone sound. Nothing leaves the local network.

## Licence

MIT, voir [LICENSE](LICENSE). Composants inclus dans le zip Windows : [Piper](https://github.com/rhasspy/piper) (MIT), espeak-ng (GPL-3.0), voix [fr_FR-siwis-medium](https://huggingface.co/rhasspy/piper-voices). Le « Test cri » (`app/assets/test-cri.flac`) : une phrase dite par cette voix, puis le cri « ahhh » de UncleSigmund sur Freesound, domaine public (CC0), [via Wikimedia Commons](https://commons.wikimedia.org/wiki/File:UncleSigmund_-_ahhh_(cc0)_(freesound).mp3).
