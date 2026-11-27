package ma.tdm.micro

/** Le cœur en Rust (telephone/rust) : capture AAudio et envoi TCP. */
object Micro {
    init { System.loadLibrary("micro_telephone") }
    external fun demarrer(port: Int)
    external fun arreter()
    /** 0 arrêté, 1 attente du PC, 2 connecté, -1 micro refusé, -2 port occupé. */
    external fun etat(): Int
    /** Code à 6 chiffres à taper dans la table avec l'adresse. */
    external fun code(): Int
    /** Crête depuis la lecture précédente, en millièmes de pleine échelle. */
    external fun crete(): Int
    /**
     * Envoie les réglages de la voie au PC. blocs : 1 anti-bruit, 2 coupe-cri, 4 limiteur.
     * apprendre : un nombre différent de pc(1) lance « Apprendre le bruit ».
     * cri, lim, porte en dixièmes de dB, reaction en dixièmes de ms.
     */
    external fun regler(blocs: Int, apprendre: Int, cri: Int, reaction: Int, lim: Int, porte: Int)
    /** Réglages reçus du PC, dans l'ordre de regler ; pc(6) vaut 1 dès que le PC en a envoyé. */
    external fun pc(i: Int): Int
}
