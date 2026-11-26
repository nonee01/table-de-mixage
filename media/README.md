# Media

| Fichier | Ce qu'on voit | Comment le refaire |
|---|---|---|
| `fenetre.png` | La fenêtre sur le hp, sortie muette : cinq voies, la Sortie et le tracé entrée / sortie de Micro 1. « Aucun signal » sous Micro 1 parce que le micro du hp est coupé dans le système. | Sur le hp : micro rallumé (`pactl set-source-mute @DEFAULT_SOURCE@ 0`), `TABLE_DE_MIXAGE_DEBUG=muet TABLE_DE_MIXAGE_CAPTURE=/tmp/fenetre.ppm ./target/release/table-de-mixage`, puis `magick /tmp/fenetre.ppm media/fenetre.png` |
