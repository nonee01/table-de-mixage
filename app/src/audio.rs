//! Le son en direct : périphériques, flux cpal, et le rappel de sortie qui fait tourner la table.
//! Le rappel de sortie ne verrouille rien, n'alloue rien : il lit des anneaux et des triples tampons.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, StreamConfig};
use moteur::table::{EntreesBloc, ReglagesTable, Table, NB_VOIES};
use moteur::{BLOC, FE};
use ringbuf::traits::{Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use triple_buffer::TripleBuffer;

use crate::sources::{LecteurMusique, Voix};
use crate::telephone::{self, Telephone};
use crate::anneau::{borner, catmull_rom, tirer_bloc, Reechantillonneur};

#[derive(Clone, Debug, PartialEq)]
pub enum SourceMicro {
    Aucun,
    ParDefaut,
    Peripherique(String),
    Telephone,
    /// L'appli « Micro table » à cette adresse (IP du téléphone, port 5005 par défaut).
    TelephoneWifi(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChoixAudio {
    pub sortie: Option<String>,
    pub micro1: SourceMicro,
    pub micro2: SourceMicro,
}

impl Default for ChoixAudio {
    fn default() -> Self {
        ChoixAudio { sortie: None, micro1: SourceMicro::ParDefaut, micro2: SourceMicro::Aucun }
    }
}

/// Ce que le son renvoie à la fenêtre, environ 190 fois par seconde.
#[derive(Clone, Copy, Debug, Default)]
pub struct Retour {
    pub cretes: [f32; NB_VOIES],
    pub master: [f32; 2],
    pub coupe: [bool; NB_VOIES],
    pub latence_ms: f32,
    /// Micro 1, Micro 2 perdus (débranchés).
    pub perdu: [bool; 2],
    pub sortie_perdue: bool,
}

pub struct LiensUi {
    pub reglages: triple_buffer::Input<ReglagesTable>,
    pub retour: triple_buffer::Output<Retour>,
    pub sonde_entree: HeapCons<f32>,
    pub sonde_sortie: HeapCons<f32>,
    /// La voie sondée, paires (avant, après) ses blocs, quand `enregistrer` est vrai ; prise par l'enregistrement en cours.
    pub enregistrement: Option<HeapCons<f32>>,
    pub enregistrer: Arc<AtomicBool>,
    pub musique: LecteurMusique,
    pub voix: Voix,
    /// Micro 1, Micro 2 : pourquoi la source choisie n'a pas pu s'ouvrir (téléphone).
    pub erreurs_micro: [Option<String>; 2],
}

pub struct Audio {
    _sortie: cpal::Stream,
    _entrees: Vec<cpal::Stream>,
    _telephones: Vec<Telephone>,
}

fn nom(d: &cpal::Device) -> String {
    d.description().map(|x| x.name().to_string()).unwrap_or_else(|_| "?".into())
}

pub fn lister_entrees() -> Vec<String> {
    cpal::default_host().input_devices().map(|it| it.map(|d| nom(&d)).collect()).unwrap_or_default()
}

pub fn lister_sorties() -> Vec<String> {
    cpal::default_host().output_devices().map(|it| it.map(|d| nom(&d)).collect()).unwrap_or_default()
}

fn trouver(nom_voulu: &str, entree: bool) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    let liste = if entree { host.input_devices() } else { host.output_devices() };
    liste
        .map_err(|e| e.to_string())?
        .find(|d| nom(d) == nom_voulu)
        .ok_or_else(|| format!("Périphérique introuvable: {nom_voulu}"))
}

/// Config f32 ou i16 à 48 kHz si possible, sinon le taux le plus proche.
fn choisir_config(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
    canaux_voulus: u16,
) -> Result<cpal::SupportedStreamConfig, String> {
    let mut candidates: Vec<_> =
        configs.filter(|c| matches!(c.sample_format(), SampleFormat::F32 | SampleFormat::I16)).collect();
    // f32 d'abord, puis le bon nombre de canaux
    candidates.sort_by_key(|c| (c.sample_format() != SampleFormat::F32, c.channels() != canaux_voulus));
    for c in &candidates {
        if let Some(s) = c.clone().try_with_sample_rate(FE as u32) {
            return Ok(s);
        }
    }
    candidates.into_iter().next().map(|c| c.with_max_sample_rate()).ok_or_else(|| "Format non pris en charge".into())
}

/// Seule la disparition du périphérique le rend « perdu » ; un craquement (xrun) ou un changement de route non.
fn est_perte(e: &cpal::Error) -> bool {
    matches!(e.kind(), cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable)
}

struct Entree {
    flux: cpal::Stream,
}

fn ouvrir_entree(
    device: &cpal::Device,
    mut prod: HeapProd<f32>,
    paquet: Arc<AtomicUsize>,
    perdu: Arc<AtomicBool>,
) -> Result<Entree, String> {
    let sc = choisir_config(device.supported_input_configs().map_err(|e| e.to_string())?, 1)?;
    let config: StreamConfig = sc.config();
    let mut reech = Reechantillonneur::new(config.sample_rate, config.channels as usize);
    let ratio = FE / config.sample_rate as f32;
    let canaux = config.channels as usize;
    let erreur = move |e: cpal::Error| {
        eprintln!("micro: {e}"); // invisible d'habitude ; lisible quand stderr est redirigé
        if est_perte(&e) {
            perdu.store(true, Ordering::Relaxed);
        }
    };
    let flux = match sc.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                reech.pousser(data, &mut prod);
                paquet.store((data.len() as f32 / canaux as f32 * ratio) as usize + 1, Ordering::Relaxed);
            },
            erreur,
            None,
        ),
        _ => {
            let mut converti = vec![0.0f32; 16_384];
            device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let n = data.len().min(converti.len());
                    for (c, v) in converti.iter_mut().zip(&data[..n]) {
                        *c = *v as f32 / 32_768.0;
                    }
                    reech.pousser(&converti[..n], &mut prod);
                    paquet.store((n as f32 / canaux as f32 * ratio) as usize + 1, Ordering::Relaxed);
                },
                erreur,
                None,
            )
        }
    }
    .map_err(|e| e.to_string())?;
    flux.play().map_err(|e| e.to_string())?;
    Ok(Entree { flux })
}

/// Tout ce que le rappel de sortie possède.
struct Moteur {
    table: Table,
    reglages: triple_buffer::Output<ReglagesTable>,
    retour: triple_buffer::Input<Retour>,
    micros: [HeapCons<f32>; 2],
    paquets: [Arc<AtomicUsize>; 2],
    perdus: [Arc<AtomicBool>; 3],
    musique: HeapCons<f32>,
    voix: HeapCons<f32>,
    sonde_e: HeapProd<f32>,
    sonde_s: HeapProd<f32>,
    enregistrement: HeapProd<f32>,
    enregistrer: Arc<AtomicBool>,
    blocs: [[f32; BLOC]; 4],
    musique_entrelacee: [f32; 2 * BLOC],
    musique_g: [f32; BLOC],
    musique_d: [f32; BLOC],
    sortie: [[f32; BLOC]; 2],
    pos: usize,
    // rééchantillonnage vers la carte quand elle n'est pas à 48 kHz
    pas: f32,
    t: f32,
    hist: [[f32; 4]; 2],
    tampon_carte: usize,
}

impl Moteur {
    fn produire_bloc(&mut self) {
        let r = *self.reglages.read();
        for k in 0..2 {
            let max = self.paquets[k].load(Ordering::Relaxed) + 2 * BLOC;
            borner(&mut self.micros[k], max);
            tirer_bloc(&mut self.micros[k], &mut self.blocs[k]);
        }
        tirer_bloc(&mut self.voix, &mut self.blocs[2]);
        tirer_bloc(&mut self.musique, &mut self.musique_entrelacee);
        for i in 0..BLOC {
            self.musique_g[i] = self.musique_entrelacee[2 * i];
            self.musique_d[i] = self.musique_entrelacee[2 * i + 1];
        }
        let [m1, m2, voix, _] = &self.blocs;
        let e = EntreesBloc { micro1: m1, micro2: m2, musique_g: &self.musique_g, musique_d: &self.musique_d, voix };
        let [g, d] = &mut self.sortie;
        self.table.traiter(&r, &e, g, d);

        let (se, ss) = self.table.sonde();
        self.sonde_e.push_slice(se);
        self.sonde_s.push_slice(ss);
        if self.enregistrer.load(Ordering::Relaxed) {
            for (a, b) in se.iter().zip(ss) {
                let _ = self.enregistrement.try_push(*a);
                let _ = self.enregistrement.try_push(*b);
            }
        }
        let latence = self.tampon_carte + self.micros[0].occupied_len() + self.table.retard(0);
        self.retour.write(Retour {
            cretes: self.table.cretes(),
            master: self.table.cretes_master(),
            coupe: std::array::from_fn(|i| self.table.coupe(i)),
            latence_ms: latence as f32 / FE * 1000.0,
            perdu: [self.perdus[0].load(Ordering::Relaxed), self.perdus[1].load(Ordering::Relaxed)],
            sortie_perdue: self.perdus[2].load(Ordering::Relaxed),
        });
        self.pos = 0;
    }

    fn trame48(&mut self) -> (f32, f32) {
        if self.pos == BLOC {
            self.produire_bloc();
        }
        let v = (self.sortie[0][self.pos], self.sortie[1][self.pos]);
        self.pos += 1;
        v
    }

    fn trame_carte(&mut self) -> (f32, f32) {
        if self.pas == 1.0 {
            return self.trame48();
        }
        while self.t >= 1.0 {
            let (g, d) = self.trame48();
            for (h, v) in self.hist.iter_mut().zip([g, d]) {
                *h = [h[1], h[2], h[3], v];
            }
            self.t -= 1.0;
        }
        let v = (catmull_rom(&self.hist[0], self.t), catmull_rom(&self.hist[1], self.t));
        self.t += self.pas;
        v
    }

    fn remplir(&mut self, data: &mut [f32], canaux: usize) {
        self.tampon_carte = data.len() / canaux;
        for trame in data.chunks_exact_mut(canaux) {
            let (g, d) = self.trame_carte();
            match trame {
                [m] => *m = 0.5 * (g + d),
                [a, b, reste @ ..] => {
                    *a = g;
                    *b = d;
                    reste.iter_mut().for_each(|v| *v = 0.0);
                }
                [] => {}
            }
        }
    }
}

fn anneau(n: usize) -> (HeapProd<f32>, HeapCons<f32>) {
    HeapRb::<f32>::new(n).split()
}

impl Audio {
    /// Ouvre la sortie et les micros choisis. Lâcher `Audio` arrête tout.
    pub fn demarrer(choix: &ChoixAudio) -> Result<(Audio, LiensUi), String> {
        let host = cpal::default_host();
        let sortie = match &choix.sortie {
            Some(n) => trouver(n, false)?,
            None => host.default_output_device().ok_or("Aucune sortie audio")?,
        };

        let perdus: [Arc<AtomicBool>; 3] = std::array::from_fn(|_| Arc::new(AtomicBool::new(false)));
        let paquets: [Arc<AtomicUsize>; 2] = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
        let mut entrees = Vec::new();
        let mut micros = Vec::new();
        let mut telephones = Vec::new();
        let mut erreurs_micro: [Option<String>; 2] = [None, None];
        for (k, source) in [&choix.micro1, &choix.micro2].into_iter().enumerate() {
            let (prod, cons) = anneau(16_384);
            micros.push(cons);
            let device = match source {
                SourceMicro::ParDefaut => host.default_input_device(),
                SourceMicro::Peripherique(n) => Some(trouver(n, true)?),
                SourceMicro::Telephone => {
                    // un téléphone absent ne coupe pas la table : l'erreur va dans la boîte de la voie
                    let t = match (telephone::trouver_adb(), telephone::trouver_serveur()) {
                        (Some(adb), Some(srv)) => Telephone::demarrer(&adb, &srv, prod, paquets[k].clone(), perdus[k].clone()),
                        _ => Err("adb ou scrcpy-server introuvable".into()),
                    };
                    match t {
                        Ok(t) => telephones.push(t),
                        Err(e) => erreurs_micro[k] = Some(e),
                    }
                    continue;
                }
                SourceMicro::TelephoneWifi(adresse) => {
                    match Telephone::wifi(adresse, prod, paquets[k].clone(), perdus[k].clone()) {
                        Ok(t) => telephones.push(t),
                        Err(e) => erreurs_micro[k] = Some(e),
                    }
                    continue;
                }
                SourceMicro::Aucun => None,
            };
            if let Some(d) = device {
                entrees.push(ouvrir_entree(&d, prod, paquets[k].clone(), perdus[k].clone())?.flux);
            }
        }

        let (reglages_in, reglages_out) = TripleBuffer::new(&ReglagesTable::default()).split();
        let (retour_in, retour_out) = TripleBuffer::new(&Retour::default()).split();
        let (sonde_e, sonde_entree) = anneau(48_000);
        let (sonde_s, sonde_sortie) = anneau(48_000);
        let (rec_p, rec_c) = anneau(4 * 48_000);
        let enregistrer = Arc::new(AtomicBool::new(false));
        let (musique_p, musique_c) = anneau(16_384);
        let (voix_p, voix_c) = anneau(16_384);

        let sc = choisir_config(sortie.supported_output_configs().map_err(|e| e.to_string())?, 2)?;
        if sc.sample_format() != SampleFormat::F32 {
            return Err("La sortie ne prend pas le format f32".into());
        }
        let mut config = sc.config();
        let canaux = config.channels as usize;
        let [micro1, micro2]: [HeapCons<f32>; 2] = micros.try_into().map_err(|_| "micros")?;
        let mut moteur = Moteur {
            table: Table::new(),
            reglages: reglages_out,
            retour: retour_in,
            micros: [micro1, micro2],
            paquets,
            perdus: perdus.clone(),
            musique: musique_c,
            voix: voix_c,
            sonde_e,
            sonde_s,
            enregistrement: rec_p,
            enregistrer: enregistrer.clone(),
            blocs: [[0.0; BLOC]; 4],
            musique_entrelacee: [0.0; 2 * BLOC],
            musique_g: [0.0; BLOC],
            musique_d: [0.0; BLOC],
            sortie: [[0.0; BLOC]; 2],
            pos: BLOC,
            pas: FE / config.sample_rate as f32,
            t: 1.0,
            hist: [[0.0; 4]; 2],
            tampon_carte: BLOC,
        };
        // un premier bloc tout de suite, pour que le rappel n'ait jamais rien à allouer
        moteur.produire_bloc();
        moteur.pos = BLOC;

        let perdu_sortie = perdus[2].clone();
        let mut construire = |taille: BufferSize, m: Moteur| {
            config.buffer_size = taille;
            let mut m = m;
            let perdu = perdu_sortie.clone();
            sortie.build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| m.remplir(data, canaux),
                move |e: cpal::Error| {
                    eprintln!("sortie: {e}");
                    if est_perte(&e) {
                        perdu.store(true, Ordering::Relaxed);
                    }
                },
                None,
            )
        };
        // un tampon de 256 trames si la carte l'accepte, sinon celui qu'elle propose
        let flux = match sc.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } if *min <= BLOC as u32 && BLOC as u32 <= *max => {
                construire(BufferSize::Fixed(BLOC as u32), moteur)
            }
            _ => construire(BufferSize::Default, moteur),
        }
        .map_err(|e| e.to_string())?;
        flux.play().map_err(|e| e.to_string())?;

        let liens = LiensUi {
            reglages: reglages_in,
            retour: retour_out,
            sonde_entree,
            sonde_sortie,
            enregistrement: Some(rec_c),
            enregistrer,
            musique: LecteurMusique::new(musique_p),
            voix: Voix::new(voix_p),
            erreurs_micro,
        };
        Ok((Audio { _sortie: flux, _entrees: entrees, _telephones: telephones }, liens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appli « Micro table » démarrée : `TDM_TELEPHONE="<ip> <code>" cargo test -p table-de-mixage enregistrement_vrai -- --ignored`
    #[test]
    #[ignore]
    fn enregistrement_vrai_du_telephone() {
        let choix = ChoixAudio { sortie: None, micro1: SourceMicro::TelephoneWifi(std::env::var("TDM_TELEPHONE").unwrap()), micro2: SourceMicro::Aucun };
        let (_audio, mut l) = Audio::demarrer(&choix).unwrap();
        assert_eq!(l.erreurs_micro[0], None);
        let mut r = ReglagesTable::default();
        r.master_db = moteur::voie::VOLUME_MIN_DB; // rien aux haut-parleurs
        l.reglages.write(r);
        let base = std::env::temp_dir().join(format!("tdm-vrai-{}.wav", std::process::id()));
        let rec = crate::enregistrement::Enregistrement::demarrer(&base, l.enregistrement.take().unwrap()).map_err(|e| e.1).unwrap();
        l.enregistrer.store(true, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_secs(3));
        l.enregistrer.store(false, Ordering::Relaxed);
        rec.arreter().1.unwrap();
        let (a, b) = crate::enregistrement::chemins(&base);
        for p in [&a, &b] {
            let v: Vec<i16> = hound::WavReader::open(p).unwrap().into_samples().map(|s| s.unwrap()).collect();
            let crete = v.iter().map(|x| x.unsigned_abs()).max().unwrap_or(0);
            eprintln!("{}: {:.2} s, crête {crete}", p.display(), v.len() as f32 / 48_000.0);
            assert!(v.len() > 2 * 48_000, "{}", v.len());
            std::fs::remove_file(p).unwrap();
        }
    }

    #[test]
    fn un_craquement_ne_perd_pas_le_micro() {
        assert!(!est_perte(&cpal::Error::new(cpal::ErrorKind::Xrun)));
        assert!(!est_perte(&cpal::Error::new(cpal::ErrorKind::DeviceChanged)));
        assert!(est_perte(&cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable)));
    }

    #[test]
    fn peripherique_inconnu_donne_une_erreur() {
        let choix = ChoixAudio { sortie: Some("n'existe pas".into()), micro1: SourceMicro::Aucun, micro2: SourceMicro::Aucun };
        match Audio::demarrer(&choix) {
            Err(e) => assert_eq!(e, "Périphérique introuvable: n'existe pas"),
            Ok(_) => panic!("aurait dû échouer"),
        }
    }
}
