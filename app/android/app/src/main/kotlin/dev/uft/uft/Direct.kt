package dev.uft.uft

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothStatusCodes
import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.location.LocationManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.SoftApConfiguration
import android.net.wifi.WifiConfiguration
import android.net.wifi.WifiManager
import android.net.wifi.WifiNetworkSpecifier
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodChannel
import java.util.UUID

/**
 * Direct link without a shared network (PLAN.md Phase 5): BLE advertise/scan and the invite GATT service, the
 * LocalOnlyHotspot the sender hosts, and joining it. [local] turns the normal network binding on (true) or off.
 * Everything runs and answers on the main thread.
 */
@Suppress("MissingPermission") // every entry point checks its permissions first, or fails with SecurityException
class Direct(private val ctx: Context, private val local: (Boolean) -> Unit) : EventChannel.StreamHandler {
    private val service = UUID.fromString("b7e3c1a0-5d2f-4e8b-9c61-2a4f8d0e7b10")
    private val invite = UUID.fromString("b7e3c1a1-5d2f-4e8b-9c61-2a4f8d0e7b10")
    private val main = Handler(Looper.getMainLooper())
    private val bt = ctx.getSystemService(BluetoothManager::class.java)
    private val cm = ctx.getSystemService(ConnectivityManager::class.java)
    private val wifi = ctx.getSystemService(WifiManager::class.java)
    private var sink: EventChannel.EventSink? = null

    override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
        sink = events
    }

    override fun onCancel(arguments: Any?) {
        sink = null
    }

    private fun emit(event: Map<String, String>) = main.post { sink?.success(event) }

    private fun has(p: String) = ctx.checkSelfPermission(p) == PackageManager.PERMISSION_GRANTED

    // --- BLE -------------------------------------------------------------------------------------------------

    /** Fingerprint (8 bytes) to advertise; set by the first bleStart, so Bluetooth coming on later restarts it. */
    private var fp: ByteArray? = null
    private var server: BluetoothGattServer? = null
    private var scanning = false
    private var advertising: AdvertiseCallback? = null
    private var advertised = false
    /** Long (prepared) invite writes, by device, until executed. */
    private val writes = HashMap<String, ByteArray>()
    /** Scanned devices by address: connecting through these keeps the address type (random) the scan saw. */
    private val seen = HashMap<String, BluetoothDevice>()

    private val state = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) {
            when (i.getIntExtra(BluetoothAdapter.EXTRA_STATE, -1)) {
                BluetoothAdapter.STATE_ON -> retry()
                BluetoothAdapter.STATE_TURNING_OFF, BluetoothAdapter.STATE_OFF -> stopBle()
            }
        }
    }

    /**
     * Advertises [fpHex] (16 hex chars) and the adapter's name, serves the invite characteristic and scans for
     * other devices. Idempotent: starts only what isn't running. The name the app passes isn't used: Android only
     * advertises the Bluetooth adapter's own name.
     */
    fun bleStart(fpHex: String, result: MethodChannel.Result?) {
        if (fp == null) {
            if (fpHex.length != 16) return result?.error("ble", "bad fingerprint: $fpHex", null) ?: Unit
            fp = fpHex.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
            ctx.registerReceiver(state, IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED))
        }
        start(result)
    }

    /** Starts BLE again if bleStart ran before and something is missing: after a permission or Bluetooth came on. */
    fun retry() {
        if (fp != null) start(null)
    }

    private fun start(result: MethodChannel.Result?) {
        fun fail(why: String) = result?.error("ble", why, null) ?: Unit
        val a = bt?.adapter ?: return fail("This device has no Bluetooth")
        if (!ctx.packageManager.hasSystemFeature(PackageManager.FEATURE_BLUETOOTH_LE)) {
            return fail("This device has no Bluetooth LE")
        }
        val perms = if (Build.VERSION.SDK_INT >= 31) {
            listOf(
                Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_ADVERTISE,
                Manifest.permission.BLUETOOTH_CONNECT,
            )
        } else {
            listOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        if (!perms.all(::has)) {
            return fail(if (Build.VERSION.SDK_INT >= 31) "Nearby devices permission not granted" else "Location permission not granted")
        }
        if (!a.isEnabled) return fail("Bluetooth is off")

        if (server == null) {
            server = bt.openGattServer(ctx, gattServer)?.apply {
                val s = BluetoothGattService(service, BluetoothGattService.SERVICE_TYPE_PRIMARY)
                s.addCharacteristic(
                    BluetoothGattCharacteristic(
                        invite, BluetoothGattCharacteristic.PROPERTY_WRITE, BluetoothGattCharacteristic.PERMISSION_WRITE,
                    ),
                )
                addService(s)
            }
        }
        if (!scanning) {
            val filter = ScanFilter.Builder().setServiceData(ParcelUuid(service), ByteArray(0)).build()
            val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_BALANCED).build()
            a.bluetoothLeScanner?.startScan(listOf(filter), settings, scan)?.also { scanning = true }
        }
        // Up to Android 11 scans find nothing while Location is off, without an error.
        val located = Build.VERSION.SDK_INT >= 31 || ctx.getSystemService(LocationManager::class.java).let {
            it.isProviderEnabled(LocationManager.GPS_PROVIDER) || it.isProviderEnabled(LocationManager.NETWORK_PROVIDER)
        }
        val unseen = if (located) "" else " Location is off, so this device can't find others."
        if (!a.isMultipleAdvertisementSupported || a.bluetoothLeAdvertiser == null) {
            return fail("This device can't advertise over Bluetooth LE: it can find others, but they can't find it.$unseen")
        }
        when {
            advertised -> if (located) result?.success(null) else fail(unseen.trim())
            advertising == null -> advertise(a, true) { why ->
                if (why == null && located) result?.success(null) else fail(listOfNotNull(why, unseen.trim().ifEmpty { null }).joinToString(" "))
            }
            // ponytail: a second bleStart while the first is still starting the advertisement answers at once.
            else -> result?.success(null)
        }
    }

    /** Service data = fingerprint, connectable; the name goes in the scan response unless it doesn't fit. */
    private fun advertise(a: BluetoothAdapter, named: Boolean, done: (String?) -> Unit) {
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_BALANCED)
            .setConnectable(true)
            .setTimeout(0)
            .build()
        // 3 (flags) + 2 + 16 (UUID) + 8 (fingerprint) = 29 of 31 bytes.
        val data = AdvertiseData.Builder().addServiceData(ParcelUuid(service), fp).build()
        val response = AdvertiseData.Builder().setIncludeDeviceName(named).build()
        val cb = object : AdvertiseCallback() {
            override fun onStartSuccess(settingsInEffect: AdvertiseSettings) {
                advertised = true
                done(null)
            }

            override fun onStartFailure(errorCode: Int) {
                advertising = null
                when (errorCode) {
                    ADVERTISE_FAILED_ALREADY_STARTED -> done(null)
                    // A long Bluetooth name: advertise without it.
                    ADVERTISE_FAILED_DATA_TOO_LARGE -> if (named) advertise(a, false, done) else done("Advertisement too large")
                    ADVERTISE_FAILED_TOO_MANY_ADVERTISERS -> done("Bluetooth LE: too many advertisers on this device")
                    ADVERTISE_FAILED_FEATURE_UNSUPPORTED -> done("This device can't advertise over Bluetooth LE")
                    else -> done("Bluetooth LE advertising failed ($errorCode)")
                }
            }
        }
        advertising = cb
        a.bluetoothLeAdvertiser?.startAdvertising(settings, data, response, cb) ?: cb.onStartFailure(-1)
    }

    private val scan = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, r: ScanResult) {
            val data = r.scanRecord?.getServiceData(ParcelUuid(service))?.takeIf { it.size >= 8 } ?: return
            seen[r.device.address] = r.device
            val name = r.scanRecord?.deviceName ?: runCatching { r.device.name }.getOrNull() ?: ""
            val hex = data.take(8).joinToString("") { "%02x".format(it) }
            emit(mapOf("type" to "seen", "id" to r.device.address, "name" to name, "fp" to hex))
        }

        override fun onScanFailed(errorCode: Int) {
            scanning = false
        }
    }

    private val gattServer = object : BluetoothGattServerCallback() {
        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice, requestId: Int, characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean, responseNeeded: Boolean, offset: Int, value: ByteArray,
        ) {
            var status = BluetoothGatt.GATT_SUCCESS
            if (characteristic.uuid != invite) {
                status = BluetoothGatt.GATT_WRITE_NOT_PERMITTED
            } else if (offset + value.size > 4096) {
                // Anyone nearby can write: an invite is ~240 bytes, so don't buffer much more.
                status = BluetoothGatt.GATT_INVALID_ATTRIBUTE_LENGTH
            } else if (preparedWrite) {
                synchronized(writes) {
                    val old = writes[device.address] ?: ByteArray(0)
                    val buf = if (old.size < offset + value.size) old.copyOf(offset + value.size) else old
                    value.copyInto(buf, offset)
                    writes[device.address] = buf
                }
            } else {
                emit(mapOf("type" to "invite", "json" to String(value, Charsets.UTF_8)))
            }
            if (responseNeeded) server?.sendResponse(device, requestId, status, offset, value)
        }

        override fun onExecuteWrite(device: BluetoothDevice, requestId: Int, execute: Boolean) {
            val buf = synchronized(writes) { writes.remove(device.address) }
            if (execute && buf != null) emit(mapOf("type" to "invite", "json" to String(buf, Charsets.UTF_8)))
            server?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
        }

        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            if (newState == BluetoothProfile.STATE_DISCONNECTED) synchronized(writes) { writes.remove(device.address) }
        }
    }

    /** Writes [json] to the invite characteristic of the device seen as [id], then disconnects. */
    fun bleInvite(id: String, json: String, result: MethodChannel.Result) {
        val a = bt?.adapter?.takeIf { it.isEnabled } ?: return result.error("ble", "Bluetooth is off", null)
        val device = seen[id] ?: runCatching { a.getRemoteDevice(id) }.getOrNull()
            ?: return result.error("ble", "bad device address: $id", null)
        val bytes = json.toByteArray(Charsets.UTF_8)
        var gatt: BluetoothGatt? = null
        var done = false
        lateinit var timeout: Runnable
        fun finish(err: String?) {
            if (done) return
            done = true
            main.removeCallbacks(timeout)
            gatt?.disconnect()
            gatt?.close()
            if (err == null) result.success(null) else result.error("ble", err, null)
        }
        timeout = Runnable { finish("Timed out sending the invite over Bluetooth") }
        val cb = object : BluetoothGattCallback() {
            override fun onConnectionStateChange(g: BluetoothGatt, status: Int, newState: Int) {
                main.post {
                    if (newState != BluetoothProfile.STATE_CONNECTED) {
                        finish("Bluetooth connection lost ($status)")
                    } else if (!g.requestMtu(517)) {
                        g.discoverServices()
                    }
                }
            }

            override fun onMtuChanged(g: BluetoothGatt, mtu: Int, status: Int) {
                // A small MTU still works: Android splits the write into a long (prepared) write.
                main.post { if (!g.discoverServices()) finish("Bluetooth: can't discover services") }
            }

            override fun onServicesDiscovered(g: BluetoothGatt, status: Int) {
                main.post {
                    val ch = g.getService(service)?.getCharacteristic(invite)
                        ?: return@post finish("That device isn't offering Swoop invites")
                    val ok = if (Build.VERSION.SDK_INT >= 33) {
                        g.writeCharacteristic(ch, bytes, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT) ==
                            BluetoothStatusCodes.SUCCESS
                    } else {
                        @Suppress("DEPRECATION")
                        ch.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
                        @Suppress("DEPRECATION")
                        ch.value = bytes
                        @Suppress("DEPRECATION")
                        g.writeCharacteristic(ch)
                    }
                    if (!ok) finish("Bluetooth: can't write the invite")
                }
            }

            override fun onCharacteristicWrite(g: BluetoothGatt, ch: BluetoothGattCharacteristic, status: Int) {
                main.post { finish(if (status == BluetoothGatt.GATT_SUCCESS) null else "Invite refused ($status)") }
            }
        }
        try {
            gatt = device.connectGatt(ctx, false, cb, BluetoothDevice.TRANSPORT_LE)
        } catch (e: SecurityException) {
            return finish(e.message ?: "Bluetooth permission not granted")
        }
        if (gatt == null) return finish("Bluetooth: can't connect")
        main.postDelayed(timeout, 20_000)
    }

    /** Stops looking for others (the app is out of sight); [retry] starts again. */
    fun pauseScan() {
        if (scanning) runCatching { bt?.adapter?.bluetoothLeScanner?.stopScan(scan) }
        scanning = false
    }

    private fun stopBle() {
        val a = bt?.adapter
        if (scanning) runCatching { a?.bluetoothLeScanner?.stopScan(scan) }
        advertising?.let { cb -> runCatching { a?.bluetoothLeAdvertiser?.stopAdvertising(cb) } }
        runCatching { server?.close() }
        scanning = false
        advertising = null
        advertised = false
        server = null
        synchronized(writes) { writes.clear() }
    }

    // --- Hotspot ---------------------------------------------------------------------------------------------

    private var reservation: WifiManager.LocalOnlyHotspotReservation? = null
    private var closed = false
    /** True while this phone hosts the hotspot. */
    val hostingHotspot get() = reservation != null
    private var hosted: Map<String, String>? = null
    private var hosting: MethodChannel.Result? = null

    /** Starts a LocalOnlyHotspot; answers {ssid, pass, sec}. Unbinds the process so sockets reach its clients. */
    fun host(result: MethodChannel.Result) {
        hosted?.let { return result.success(it) }
        if (hosting != null) return result.error("wifi", "The hotspot is already starting", null)
        hosting = result
        val cb = object : WifiManager.LocalOnlyHotspotCallback() {
            override fun onStarted(r: WifiManager.LocalOnlyHotspotReservation) {
                // The activity went away while it was starting: nobody would ever close it.
                if (closed) return r.close()
                reservation = r
                val creds = if (Build.VERSION.SDK_INT >= 30) {
                    val c = r.softApConfiguration
                    @Suppress("DEPRECATION")
                    val ssid = c.ssid
                    val sec = when (c.securityType) {
                        SoftApConfiguration.SECURITY_TYPE_WPA3_SAE_TRANSITION -> "wpa3t"
                        SoftApConfiguration.SECURITY_TYPE_WPA3_SAE -> "wpa3"
                        else -> "wpa2"
                    }
                    mapOf("ssid" to ssid, "pass" to c.passphrase, "sec" to sec)
                } else {
                    @Suppress("DEPRECATION")
                    val c = r.wifiConfiguration!!
                    @Suppress("DEPRECATION")
                    mapOf("ssid" to c.SSID, "pass" to c.preSharedKey, "sec" to "wpa2")
                }.mapValues { (it.value ?: "").removeSurrounding("\"") }
                hosted = creds
                local(false)
                hosting?.success(creds)
                hosting = null
            }

            override fun onStopped() {
                reservation = null
                hosted = null
                local(true)
            }

            override fun onFailed(reason: Int) {
                val why = when (reason) {
                    ERROR_NO_CHANNEL -> "no free Wi-Fi channel"
                    ERROR_INCOMPATIBLE_MODE -> "Wi-Fi is in an incompatible mode (turn off the regular hotspot or tethering)"
                    ERROR_TETHERING_DISALLOWED -> "hotspots are disabled on this device"
                    else -> "it failed ($reason)"
                }
                hosting?.error("wifi", "Can't start a hotspot: $why", null)
                hosting = null
            }
        }
        try {
            wifi.startLocalOnlyHotspot(cb, main)
        } catch (e: Exception) {
            // SecurityException: no permission, or Location is off (up to Android 12); IllegalStateException: Wi-Fi.
            hosting = null
            result.error("wifi", "Can't start a hotspot: ${e.message}", null)
        }
    }

    fun stopHost() {
        reservation?.close()
        reservation = null
        hosted = null
        local(true)
    }

    // --- Join ------------------------------------------------------------------------------------------------

    private var request: ConnectivityManager.NetworkCallback? = null
    private var added = -1
    private var joining: MethodChannel.Result? = null
    private val poll = Runnable { pollLegacy() }
    private var pollUntil = 0L

    /** Joins the hotspot and binds the process to it; answers once it's usable. Android 10+ asks the user once. */
    fun join(ssid: String, pass: String, sec: String, result: MethodChannel.Result) {
        leave(false)
        joining = result
        if (Build.VERSION.SDK_INT >= 29) {
            val spec = WifiNetworkSpecifier.Builder().setSsid(ssid).apply {
                if (sec == "wpa3") setWpa3Passphrase(pass) else setWpa2Passphrase(pass)
            }.build()
            val req = NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .setNetworkSpecifier(spec)
                .build()
            val cb = object : ConnectivityManager.NetworkCallback() {
                override fun onAvailable(n: Network) {
                    local(false)
                    cm.bindProcessToNetwork(n)
                    answer(null)
                }

                override fun onLost(n: Network) {
                    if (cm.boundNetworkForProcess == n) cm.bindProcessToNetwork(null)
                }

                override fun onUnavailable() {
                    request = null
                    answer("Couldn't join $ssid: declined, not found or timed out")
                }
            }
            request = cb
            try {
                cm.requestNetwork(req, cb, main, 60_000)
            } catch (e: Exception) {
                request = null
                answer("Couldn't join $ssid: ${e.message}")
            }
        } else {
            if (sec == "wpa3") return answer("WPA3-only hotspots need Android 10 or newer")
            @Suppress("DEPRECATION")
            val conf = WifiConfiguration().apply {
                SSID = "\"$ssid\""
                preSharedKey = "\"$pass\""
                allowedKeyManagement.set(WifiConfiguration.KeyMgmt.WPA_PSK)
            }
            @Suppress("DEPRECATION")
            added = wifi.addNetwork(conf)
            @Suppress("DEPRECATION")
            if (added == -1 || !wifi.enableNetwork(added, true)) return answer("Couldn't join $ssid: Wi-Fi refused it")
            pollUntil = System.currentTimeMillis() + 60_000
            main.post(poll)
        }
    }

    /** Android 8–9: wait until Wi-Fi is on the added network with an address, then bind to it. */
    @Suppress("DEPRECATION")
    private fun pollLegacy() {
        val info = wifi.connectionInfo
        if (info != null && info.ipAddress != 0 && info.networkId == added) {
            val n = cm.allNetworks.firstOrNull { cm.getNetworkCapabilities(it)?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true }
            if (n != null) {
                local(false)
                cm.bindProcessToNetwork(n)
                return answer(null)
            }
        }
        if (System.currentTimeMillis() > pollUntil) return answer("Couldn't join the hotspot: timed out")
        main.postDelayed(poll, 500)
    }

    private fun answer(err: String?) {
        val r = joining ?: return
        joining = null
        if (err == null) r.success(null) else r.error("wifi", err, null)
    }

    /** Undoes join; with [restore] the normal network binding comes back. */
    fun leave(restore: Boolean = true) {
        main.removeCallbacks(poll)
        answer("Cancelled")
        request?.let { runCatching { cm.unregisterNetworkCallback(it) } }
        request = null
        if (added != -1) {
            @Suppress("DEPRECATION")
            wifi.removeNetwork(added)
            // ponytail: Wi-Fi goes back to its saved networks on its own schedule; reconnect() just nudges it.
            @Suppress("DEPRECATION")
            wifi.reconnect()
            added = -1
        }
        if (restore) local(true)
    }

    /** Everything off, for the activity going away. Leaves the network binding to the caller. */
    fun close() {
        closed = true
        stopBle()
        if (fp != null) runCatching { ctx.unregisterReceiver(state) }
        fp = null
        reservation?.close()
        reservation = null
        hosted = null
        leave(false)
    }
}
