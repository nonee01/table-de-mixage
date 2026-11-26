# Media

| Fichier | Ce qu'on voit | Comment le refaire |
|---|---|---|
| `fenetre.png` | La fenêtre sur le hp, sortie muette : cinq voies et la Sortie. « Aucun signal » sous Micro 1 parce que le micro du hp est coupé dans le système. | Sur le hp : `TABLE_DE_MIXAGE_DEBUG=muet TABLE_DE_MIXAGE_CAPTURE=/tmp/fenetre.ppm ./target/release/table-de-mixage`, puis `magick /tmp/fenetre.ppm media/fenetre.png` |
