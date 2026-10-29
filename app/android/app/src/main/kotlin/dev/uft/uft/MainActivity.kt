package dev.uft.uft

import android.Manifest
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.os.PowerManager
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import android.provider.OpenableColumns
import android.provider.Settings
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel
import java.io.File

/** The platform side of the app: what the Rust core can't do from inside an Android process. */
class MainActivity : FlutterActivity() {
    private val pick = 1
    private var picking: Pair<MethodChannel.Result, Boolean>? = null
    /** Picked files handed to Rust as /proc/self/fd/N, open until the send ends. */
    private val fds = HashMap<String, ParcelFileDescriptor>()
    private var multicast: WifiManager.MulticastLock? = null
    private var wifiLocks: List<WifiManager.WifiLock> = emptyList()
    private var wake: PowerManager.WakeLock? = null
    private var network: ConnectivityManager.NetworkCallback? = null
    private val direct by lazy { Direct(applicationContext, ::localBinding) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val perms = mutableListOf<String>()
        if (Build.VERSION.SDK_INT <= 29) perms += Manifest.permission.WRITE_EXTERNAL_STORAGE
        if (Build.VERSION.SDK_INT >= 33) perms += Manifest.permission.POST_NOTIFICATIONS
        // Direct link (BLE + hotspot). Up to 12 the hotspot needs precise location; up to 11 so does BLE scanning.
        if (Build.VERSION.SDK_INT <= 32) {
            perms += listOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION)
        }
        if (Build.VERSION.SDK_INT >= 31) {
            perms += listOf(
                Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_ADVERTISE,
                Manifest.permission.BLUETOOTH_CONNECT,
            )
        }
        if (Build.VERSION.SDK_INT >= 33) perms += Manifest.permission.NEARBY_WIFI_DEVICES
        if (perms.isNotEmpty()) requestPermissions(perms.toTypedArray(), 0)
        // Discovery queries arrive by multicast and broadcast, which Wi-Fi drops without this.
        multicast = wifi().createMulticastLock("uft").apply { setReferenceCounted(false); acquire() }
        bindToLocalNetwork()
        // Keeps the process alive, and receiving, while the app is in the background.
        startForegroundService(Intent(this, TransferService::class.java))
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        direct.retry()
    }

    override fun onDestroy() {
        busy(false)
        direct.close()
        multicast?.release()
        network?.let { getSystemService(ConnectivityManager::class.java).unregisterNetworkCallback(it) }
        fds.values.forEach { it.close() }
        // ponytail: closing the app ends running transfers; keep the service up until they finish if that bites.
        if (isFinishing) stopService(Intent(this, TransferService::class.java))
        super.onDestroy()
    }

    override fun configureFlutterEngine(engine: FlutterEngine) {
        super.configureFlutterEngine(engine)
        MethodChannel(engine.dartExecutor.binaryMessenger, "uft").setMethodCallHandler { call, result ->
            when (call.method) {
                "info" -> result.success(
                    mapOf(
                        "name" to deviceName(),
                        "downloads" to File(
                            Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "uft",
                        ).path,
                    ),
                )
                "pick" -> pick(call.arguments as Boolean, result)
                "release" -> {
                    synchronized(fds) { (call.arguments as List<*>).forEach { fds.remove(it)?.close() } }
                    result.success(null)
                }
                "busy" -> {
                    busy(call.arguments as Boolean)
                    result.success(null)
                }
                "bleStart" -> direct.bleStart(call.argument<String>("fp")!!, result)
                "bleInvite" -> direct.bleInvite(call.argument<String>("id")!!, call.argument<String>("json")!!, result)
                "host" -> direct.host(result)
                "stopHost" -> {
                    direct.stopHost()
                    result.success(null)
                }
                "join" -> direct.join(
                    call.argument<String>("ssid")!!, call.argument<String>("pass")!!, call.argument<String>("sec")!!,
                    result,
                )
                "leave" -> {
                    direct.leave()
                    result.success(null)
                }
                "link" -> result.success(link())
                else -> result.notImplemented()
            }
        }
        EventChannel(engine.dartExecutor.binaryMessenger, "uft/ble").setStreamHandler(direct)
    }

    private fun wifi() = applicationContext.getSystemService(WifiManager::class.java)

    /** This device's local link for diagnostics (core `link::Link`): {kind, mhz, mbps, dbm}, or null. */
    private fun link(): Map<String, Any?>? {
        // ponytail: hosting a LocalOnlyHotspot, the link is the phone's own AP; WifiManager doesn't report its rate.
        if (direct.hostingHotspot) return null
        val cm = getSystemService(ConnectivityManager::class.java)
        val caps = cm.getNetworkCapabilities(cm.boundNetworkForProcess ?: cm.activeNetwork) ?: return null
        if (caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) {
            return mapOf("kind" to "ethernet", "mhz" to null, "mbps" to null, "dbm" to null)
        }
        if (!caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI)) return null
        @Suppress("DEPRECATION")
        val w = wifi().connectionInfo ?: return null
        val mbps = if (Build.VERSION.SDK_INT >= 29) maxOf(w.linkSpeed, w.txLinkSpeedMbps, w.rxLinkSpeedMbps) else w.linkSpeed
        return mapOf(
            "kind" to "wifi",
            "mhz" to w.frequency.takeIf { it > 0 },
            "mbps" to mbps.takeIf { it > 0 },
            "dbm" to w.rssi.takeIf { it in -126..-1 },
        )
    }

    private fun deviceName(): String =
        Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME)?.takeIf { it.isNotBlank() }
            ?: Build.MODEL

    /**
     * Sends this app's traffic over Wi-Fi or Ethernet even when that network has no internet and Android
     * routes everything else over mobile data. Hotspot clients are reachable either way.
     */
    private fun bindToLocalNetwork() {
        val cm = getSystemService(ConnectivityManager::class.java)
        val req = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        // ponytail: binds to whichever local network came last; the phone hosting a hotspot while also on
        // Wi-Fi is not handled.
        network = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(n: Network) {
                cm.bindProcessToNetwork(n)
            }

            override fun onLost(n: Network) {
                if (cm.boundNetworkForProcess == n) cm.bindProcessToNetwork(null)
            }
        }.also { cm.registerNetworkCallback(req, it) }
    }

    /** The normal binding ([bindToLocalNetwork]) on, or off and unbound: for hosting or joining a hotspot. */
    private fun localBinding(on: Boolean) {
        val cm = getSystemService(ConnectivityManager::class.java)
        network?.let { cm.unregisterNetworkCallback(it) }
        network = null
        cm.bindProcessToNetwork(null)
        if (on) bindToLocalNetwork()
    }

    /** While transferring: CPU on and Wi-Fi out of power save, also with the screen off. */
    private fun busy(on: Boolean) {
        if (on && wake == null) {
            wake = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "uft:transfer").apply { acquire(6 * 3600 * 1000L) }
            @Suppress("DEPRECATION")
            val modes = listOfNotNull(
                WifiManager.WIFI_MODE_FULL_HIGH_PERF,
                if (Build.VERSION.SDK_INT >= 29) WifiManager.WIFI_MODE_FULL_LOW_LATENCY else null,
            )
            wifiLocks = modes.map { wifi().createWifiLock(it, "uft").apply { acquire() } }
        } else if (!on) {
            wake?.takeIf { it.isHeld }?.release()
            wake = null
            wifiLocks.forEach { if (it.isHeld) it.release() }
            wifiLocks = emptyList()
        }
    }

    private fun pick(folder: Boolean, result: MethodChannel.Result) {
        picking?.first?.success(emptyList<List<String>>())
        picking = result to folder
        val i = if (folder) {
            Intent(Intent.ACTION_OPEN_DOCUMENT_TREE)
        } else {
            Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("*/*")
                .putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
        }
        @Suppress("DEPRECATION")
        startActivityForResult(i, pick)
    }

    @Deprecated("FlutterActivity has no ActivityResult API")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != pick) return
        val (result, folder) = picking ?: return
        picking = null
        if (resultCode != RESULT_OK || data == null) return result.success(emptyList<List<String>>())
        // Off the UI thread: a big folder means many queries.
        Thread {
            try {
                val out = if (folder) tree(data.data!!) else uris(data).map { open(it, name(it)) }
                runOnUiThread { result.success(out) }
            } catch (e: Exception) {
                runOnUiThread { result.error("pick", e.message, null) }
            }
        }.start()
    }

    private fun uris(data: Intent): List<Uri> =
        data.clipData?.let { c -> (0 until c.itemCount).map { c.getItemAt(it).uri } } ?: listOfNotNull(data.data)

    private fun name(uri: Uri): String =
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: uri.lastPathSegment ?: "file"

    /** [path, name]: the descriptor as a path the Rust core reads, and the name to send it under. */
    private fun open(uri: Uri, name: String): List<String> {
        val pfd = contentResolver.openFileDescriptor(uri, "r") ?: error("can't open $name")
        val path = "/proc/self/fd/${pfd.fd}"
        synchronized(fds) { fds[path] = pfd }
        return listOf(path, name)
    }

    /** Every file under a picked folder, named `folder/sub/file`. */
    private fun tree(root: Uri): List<List<String>> {
        val rootId = DocumentsContract.getTreeDocumentId(root)
        val out = mutableListOf<List<String>>()
        // ponytail: one open descriptor per file; folders with more files than the fd limit (~32k) fail.
        fun walk(id: String, rel: String) {
            val children = DocumentsContract.buildChildDocumentsUriUsingTree(root, id)
            val cols = arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE)
            contentResolver.query(children, cols, null, null, null)?.use { c ->
                while (c.moveToNext()) {
                    val path = "$rel/${c.getString(1)}"
                    if (c.getString(2) == Document.MIME_TYPE_DIR) {
                        walk(c.getString(0), path)
                    } else {
                        out += open(DocumentsContract.buildDocumentUriUsingTree(root, c.getString(0)), path)
                    }
                }
            }
        }
        walk(rootId, name(DocumentsContract.buildDocumentUriUsingTree(root, rootId)))
        return out
    }
}
