// Unchanged app dependencies for the focused service/publisher API build.
package dev.orgtree.hubchat
import android.content.Context
import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Intent
import androidx.work.Worker
import androidx.work.WorkerParameters
class MainActivity : Activity() { companion object { const val inFront = false } }
object R { object drawable { const val ic_stat_hubchat = 1 } }
object TransferService {
  const val running = false
  fun work(context: Context, on: Boolean) {}
  fun backgroundHint() = ""
  fun coverageChanged(context: Context) {}
}
object PushController {
  fun cachedActive(context: Context) = false
  fun active(context: Context) = false
  fun state(context: Context) = "{}"
  fun set(context: Context, on: Boolean, distributor: String) = ""
  fun refresh(context: Context) {}
}
object DownloadPublisher {
  fun publish(context: Context, key: String, path: String, name: String) = ""
  fun abandoned(context: Context, key: String, source: String, root: String) {}
  fun committed(context: Context, key: String) {}
  fun failure(error: Exception) = ""
}
class CheckWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork() = Result.success()
}
class InstallReceiver : BroadcastReceiver() { override fun onReceive(context: Context, intent: Intent) {} }
