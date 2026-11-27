package ma.tdm.micro

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.pm.PackageManager
import android.graphics.Canvas
import android.graphics.Paint
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.TypedValue
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.SeekBar
import android.widget.Switch
import android.widget.TextView
import java.net.Inet4Address
import java.net.NetworkInterface
import kotlin.math.log10
import kotlin.math.max

const val PORT = 5005

/** Crête en millièmes de pleine échelle vers 0..1 sur une échelle de -60 à 0 dBFS. */
fun hauteur(c: Float): Float = if (c <= 1f) 0f else ((20 * log10(c / 1000f) + 60) / 60).coerceIn(0f, 1f)

/** Le son des dernières secondes, une barre par lecture de la crête. */
class Trace(ctx: Context, couleur: Int) : View(ctx) {
    private val pics = FloatArray(150)
    private var i = 0
    private val barre = Paint().apply { color = couleur }

    fun ajouter(c: Float) { pics[i] = hauteur(c); i = (i + 1) % pics.size; invalidate() }

    override fun onDraw(c: Canvas) {
        val pas = width.toFloat() / pics.size
        val y = height / 2f
        barre.strokeWidth = pas * 0.6f
        for (k in pics.indices) {
            val h = max(pics[(i + k) % pics.size] * y, 1f)
            val x = (k + 0.5f) * pas
            c.drawLine(x, y - h, x, y + h, barre)
        }
    }
}

/** Un curseur de réglage : indice dans pc(), bornes et unité, valeurs en dixièmes. */
class Curseur(val nom: String, val indice: Int, val min: Int, val max: Int, val unite: String)

class MainActivity : Activity() {
    private lateinit var adresse: TextView
    private lateinit var code: TextView
    private lateinit var etat: TextView
    private lateinit var trace: Trace
    private lateinit var niveau: ProgressBar
    private lateinit var db: TextView
    private lateinit var bouton: Button
    private lateinit var aide: TextView
    private val boucle = Handler(Looper.getMainLooper())
    private var tour = 0
    private var vu = Int.MIN_VALUE

    private val curseurs = listOf(
        Curseur("Seuil coupe-cri", 2, -400, 0, "dB"),
        Curseur("Réaction coupe-cri", 3, 10, 150, "ms"),
        Curseur("Seuil limiteur", 4, -400, 0, "dB"),
        Curseur("Porte anti-bruit", 5, -800, -200, "dB"),
    )
    private val blocs = listOf("Anti-bruit" to 1, "Coupe-cri" to 2, "Limiteur" to 4)
    private val interrupteurs = mutableListOf<Switch>()
    private val barres = mutableListOf<SeekBar>()
    private val reglables = mutableListOf<View>()
    private var synchro = false
    private var modifie = 0L
    private lateinit var verrou: WifiManager.WifiLock

    private fun dp(x: Number) = (x.toFloat() * resources.displayMetrics.density).toInt()

    private fun texte(t: String, style: Int) = TextView(this).apply { text = t; setTextAppearance(style) }

    /** Titre de section, celui des écrans de réglages du système. */
    private fun section(t: String) = TextView(this, null, android.R.attr.listSeparatorTextViewStyle).apply {
        text = t
        layoutParams = LinearLayout.LayoutParams(-1, -2).apply { topMargin = dp(16) }
    }

    /** Une ligne avec ses marges standard. */
    private fun ligne(vararg vues: View) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(16), dp(8), dp(16), dp(8))
        vues.forEach { addView(it) }
    }

    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        // shortcut: l'écran reste allumé au lieu d'un service au premier plan ; en faire un si l'appli doit tourner écran éteint
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        // la radio Wi-Fi ne somnole pas entre deux paquets (appli à l'écran) : moins de trous dans le son
        val mode = if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else WifiManager.WIFI_MODE_FULL_HIGH_PERF
        verrou = (applicationContext.getSystemService(WIFI_SERVICE) as WifiManager).createWifiLock(mode, "micro-table").apply { acquire() }
        val accent = TypedValue().also { theme.resolveAttribute(android.R.attr.colorAccent, it, true) }.data
        val petit = android.R.style.TextAppearance_DeviceDefault_Small
        val grand = android.R.style.TextAppearance_DeviceDefault_Large

        adresse = texte("", grand)
        code = texte("", grand)
        etat = texte("", android.R.style.TextAppearance_Material_Subhead)
        bouton = Button(this).apply { setOnClickListener { basculer() } }
        trace = Trace(this, accent).apply { layoutParams = LinearLayout.LayoutParams(-1, dp(80)) }
        niveau = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply { max = 600 }
        db = texte("", petit)

        setContentView(ScrollView(this).apply {
            addView(LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(0, 0, 0, dp(16))
                addView(section("Connexion"))
                addView(ligne(texte("Dans la table de mixage : Micro 1 ou 2 → Téléphone (Wi-Fi), puis tapez l'adresse et le code.", petit)))
                addView(ligne(texte("Adresse", petit), adresse))
                addView(ligne(texte("Code", petit), code))
                addView(ligne(texte("État", petit), etat))
                addView(ligne(bouton))
                addView(section("Son du micro"))
                addView(ligne(trace, niveau, db))
                addView(filtres(petit))
            })
        })
        rafraichir()
    }

    /** Les filtres de la voie du téléphone, réglés sur le PC. */
    private fun filtres(petit: Int): View {
        val c = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        c.addView(section("Filtres de cette voie (sur le PC)"))
        aide = texte("Disponibles une fois le téléphone connecté à la table.", petit)
        c.addView(ligne(aide))
        for ((nom, _) in blocs) {
            val s = Switch(this).apply {
                text = nom
                setTextAppearance(android.R.style.TextAppearance_Material_Subhead)
                setOnCheckedChangeListener { _, _ -> if (!synchro) envoyer() }
            }
            interrupteurs += s; reglables += s; c.addView(ligne(s))
        }
        for (k in curseurs) {
            val valeur = texte("", petit)
            val barre = SeekBar(this).apply {
                min = k.min; max = k.max
                setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
                    override fun onProgressChanged(s: SeekBar, v: Int, parUtilisateur: Boolean) {
                        valeur.text = "%.1f %s".format(v / 10f, k.unite)
                        if (parUtilisateur) envoyer()
                    }
                    override fun onStartTrackingTouch(s: SeekBar) {}
                    override fun onStopTrackingTouch(s: SeekBar) {}
                })
            }
            barres += barre; reglables += barre
            c.addView(ligne(texte(k.nom, android.R.style.TextAppearance_Material_Subhead), valeur, barre))
        }
        val apprendre = Button(this).apply {
            text = "Apprendre le bruit"
            setOnClickListener { envoyer((Micro.pc(1) + 1) and 0xff) }
        }
        reglables += apprendre
        c.addView(ligne(apprendre, texte("Gardez le silence 1 s : la table apprend le bruit de la pièce et allume l'anti-bruit.", petit)))
        return c
    }

    /** Envoie l'état des contrôles au PC ; apprendre vaut le compteur du PC pour ne rien lancer. */
    private fun envoyer(apprendre: Int = Micro.pc(1)) {
        modifie = SystemClock.uptimeMillis()
        val masque = blocs.indices.sumOf { if (interrupteurs[it].isChecked) blocs[it].second else 0 }
        val v = barres.map { it.progress }
        Micro.regler(masque, apprendre, v[0], v[1], v[2], v[3])
    }

    /** Recopie les réglages reçus du PC, sauf juste après un geste sur le téléphone. */
    private fun recevoir() {
        if (SystemClock.uptimeMillis() - modifie < 600) return
        synchro = true
        val masque = Micro.pc(0)
        blocs.forEachIndexed { k, (_, bit) -> interrupteurs[k].isChecked = masque and bit != 0 }
        curseurs.forEachIndexed { k, c -> barres[k].progress = Micro.pc(c.indice) }
        synchro = false
    }

    private fun basculer() {
        if (Micro.etat() > 0) return Micro.arreter()
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), 1)
        } else {
            Micro.demarrer(PORT)
        }
    }

    override fun onRequestPermissionsResult(code: Int, perms: Array<out String>, res: IntArray) {
        if (res.firstOrNull() == PackageManager.PERMISSION_GRANTED) Micro.demarrer(PORT)
        else etat.text = "Autorisez le micro dans les réglages du téléphone"
    }

    /** Les adresses IPv4 du téléphone (Wi-Fi et partage de connexion). */
    private fun adresses(): List<String> =
        NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.isUp && !it.isLoopback && !it.isVirtual }
            .filter { i -> listOf("tun", "rmnet", "ccmni").none { i.name.startsWith(it) } } // ni VPN ni données mobiles : le PC ne les joint pas
            .flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>()
            .map { it.hostAddress ?: "" }

    private fun rafraichir() {
        val e = Micro.etat()
        val c = Micro.crete().toFloat()
        trace.ajouter(c)
        niveau.progress = (hauteur(c) * 600).toInt()
        db.text = if (c <= 1f) "en dessous de -60 dB" else "%.0f dB".format(20 * log10(c / 1000f))
        if (tour++ % 30 == 0) { // l'adresse une fois par seconde
            val ips = adresses()
            adresse.text = if (ips.isEmpty()) "Pas de Wi-Fi : allumez le Wi-Fi ou le partage de connexion" else ips.joinToString("\n")
        }
        val n = Micro.code()
        val t = if (e > 0) "%03d %03d".format(n / 1000, n % 1000) else "affiché après Démarrer"
        if (code.text != t) code.text = t
        if (e != vu) {
            vu = e
            bouton.text = if (e > 0) "Arrêter" else "Démarrer"
            etat.text = when (e) {
                1 -> "En attente de la table (port $PORT)"
                2 -> "Connecté : le micro part vers la table"
                -1 -> "Le micro ne s'ouvre pas : une autre appli l'utilise ?"
                -2 -> "Le port $PORT est déjà pris : redémarrez l'appli"
                else -> "Arrêté"
            }
        }
        val lie = e == 2 && Micro.pc(6) == 1
        if (lie) recevoir()
        aide.visibility = if (lie) View.GONE else View.VISIBLE
        reglables.forEach { it.isEnabled = lie }
        boucle.postDelayed(::rafraichir, 33)
    }

    override fun onDestroy() {
        boucle.removeCallbacksAndMessages(null)
        Micro.arreter()
        verrou.release()
        super.onDestroy()
    }
}
