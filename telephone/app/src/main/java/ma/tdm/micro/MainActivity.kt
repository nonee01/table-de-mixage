package ma.tdm.micro

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.graphics.Color
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import java.net.Inet4Address
import java.net.NetworkInterface

const val PORT = 5005

class MainActivity : Activity() {
    private lateinit var etat: TextView
    private lateinit var adresse: TextView
    private lateinit var niveau: ProgressBar
    private lateinit var bouton: Button
    private val boucle = Handler(Looper.getMainLooper())

    override fun onCreate(b: Bundle?) {
        super.onCreate(b)
        // shortcut: l'écran reste allumé au lieu d'un service au premier plan ; en faire un si l'appli doit tourner écran éteint
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val p = (24 * resources.displayMetrics.density).toInt()
        adresse = TextView(this).apply { textSize = 28f; gravity = Gravity.CENTER; setTextColor(Color.BLACK) }
        etat = TextView(this).apply { textSize = 18f; gravity = Gravity.CENTER; setPadding(0, p, 0, p) }
        niveau = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply { max = 1000 }
        bouton = Button(this).apply { textSize = 20f; setOnClickListener { basculer() } }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setBackgroundColor(Color.WHITE)
            setPadding(p, p, p, p)
            addView(TextView(this@MainActivity).apply {
                text = "Dans la table de mixage, Micro 1 ou 2 → Téléphone (Wi-Fi), puis tapez :"
                textSize = 16f; gravity = Gravity.CENTER; setTextColor(Color.DKGRAY)
            })
            addView(adresse)
            addView(etat)
            addView(niveau)
            addView(bouton)
        })
        rafraichir()
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
        else etat.text = "Autorisez le micro dans les réglages"
    }

    /** Les adresses IPv4 du téléphone (Wi-Fi et partage de connexion). */
    private fun adresses(): List<String> =
        NetworkInterface.getNetworkInterfaces().toList()
            .filter { it.isUp && !it.isLoopback && !it.isVirtual && !it.name.startsWith("tun") } // pas l'adresse d'un VPN
            .flatMap { it.inetAddresses.toList() }
            .filterIsInstance<Inet4Address>()
            .map { it.hostAddress ?: "" }

    private fun rafraichir() {
        val e = Micro.etat()
        val ips = adresses()
        adresse.text = if (ips.isEmpty()) "Pas de réseau" else ips.joinToString("\n")
        etat.text = when (e) {
            1 -> "En attente du PC (port $PORT)"
            2 -> "Connecté : le micro part vers la table"
            -1 -> "Le micro ne s'ouvre pas"
            -2 -> "Port $PORT déjà pris"
            else -> "Arrêté"
        }
        etat.setTextColor(if (e < 0) Color.RED else if (e == 2) Color.rgb(0, 128, 0) else Color.BLACK)
        niveau.progress = Micro.crete()
        bouton.text = if (e > 0) "Arrêter" else "Démarrer"
        boucle.postDelayed(::rafraichir, 100)
    }

    override fun onDestroy() {
        boucle.removeCallbacksAndMessages(null)
        Micro.arreter()
        super.onDestroy()
    }
}
