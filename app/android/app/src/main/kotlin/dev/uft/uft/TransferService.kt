package dev.uft.uft

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build

/** Foreground service: only there so Android keeps the app running (and receiving) in the background. */
class TransferService : Service() {
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("uft", "Receiving", NotificationManager.IMPORTANCE_LOW))
        val n = notification(this, null, 0, 0)
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(1, n)
        }
        running = true
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        running = false
        super.onDestroy()
    }

    /** Android 15+: data-sync services get ~6 h a day. */
    override fun onTimeout(startId: Int, fgsType: Int) = stopSelf()

    companion object {
        var running = false
            private set

        /** The ongoing notification shows [title] with a done/total bar, or the plain text when [title] is null. */
        fun progress(ctx: Context, title: String?, done: Long, total: Long) {
            if (!running) return
            ctx.getSystemService(NotificationManager::class.java).notify(1, notification(ctx, title, done, total))
        }

        private fun notification(ctx: Context, title: String?, done: Long, total: Long): Notification {
            val open = PendingIntent.getActivity(
                ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
            )
            val b = Notification.Builder(ctx, "uft")
                .setSmallIcon(android.R.drawable.stat_sys_download_done)
                .setContentTitle("Swoop")
                .setContentText(title ?: "Visible to nearby devices")
                .setContentIntent(open)
                .setOngoing(true)
            // Per mille: the bar takes Ints, and totals pass 2 GB.
            if (title != null) {
                b.setProgress(1000, if (total > 0) (done * 1000 / total).coerceIn(0, 1000).toInt() else 0, total <= 0)
            }
            return b.build()
        }
    }
}
