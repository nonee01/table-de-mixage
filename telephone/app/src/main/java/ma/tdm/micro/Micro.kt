package ma.tdm.micro

/** Le cœur en Rust (telephone/rust) : capture AAudio et envoi TCP. */
object Micro {
    init { System.loadLibrary("micro_telephone") }
    external fun demarrer(port: Int)
    external fun arreter()
    /** 0 arrêté, 1 attente du PC, 2 connecté, -1 micro refusé, -2 port occupé. */
    external fun etat(): Int
    /** Crête du dernier paquet, en millièmes de pleine échelle. */
    external fun crete(): Int
}
