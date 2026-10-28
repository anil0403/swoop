package dev.uft.uft

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build

/** Foreground service: only there so Android keeps the app running (and receiving) in the background. */
class TransferService : Service() {
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("uft", "Receiving", NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = Notification.Builder(this, "uft")
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle("uft")
            .setContentText("Visible to nearby devices")
            .setContentIntent(open)
            .setOngoing(true)
            .build()
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(1, n)
        }
        return START_NOT_STICKY
    }

    /** Android 15+: data-sync services get ~6 h a day. */
    override fun onTimeout(startId: Int, fgsType: Int) = stopSelf()
}
