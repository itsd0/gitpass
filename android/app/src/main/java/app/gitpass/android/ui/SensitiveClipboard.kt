package app.gitpass.android.ui

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PersistableBundle

private val mainHandler = Handler(Looper.getMainLooper())

/** Copies [text] as a sensitive clip and clears it after 60s if it is still there. */
fun copySensitive(context: Context, label: String, text: String) {
    val clipboard = context.getSystemService(ClipboardManager::class.java) ?: return
    val clip = ClipData.newPlainText(label, text)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        val extras = PersistableBundle()
        extras.putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
        clip.description.extras = extras
    }
    clipboard.setPrimaryClip(clip)
    val copied = text
    mainHandler.postDelayed({
        val current = clipboard.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text?.toString()
        if (current == copied) {
            clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
        }
    }, 60_000)
}
