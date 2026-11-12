package app.gitpass.android.passkey

import android.content.Context
import androidx.annotation.RequiresApi
import androidx.annotation.VisibleForTesting
import androidx.credentials.provider.CallingAppInfo
import app.gitpass.android.R

/** Maps Android's [CallingAppInfo] to a [CallerIdentity] (#477). */
@RequiresApi(34)
object PasskeyCallers {
    /**
     * Privileged browsers allowed to name a web origin: Google's published
     * list (gstatic.com/gpm-passkeys-privileged-apps/apps.json), bundled.
     */
    @Volatile private var allowlist: String? = null
    @VisibleForTesting @Volatile var allowlistOverride: String? = null

    fun allowlist(context: Context): String = allowlistOverride ?: allowlist ?: context.resources.openRawResource(R.raw.privileged_browsers)
        .use { it.readBytes().toString(Charsets.UTF_8) }.also { allowlist = it }

    fun identify(context: Context, info: CallingAppInfo?): CallerIdentity? {
        info ?: return null
        val origin = try { info.getOrigin(allowlist(context)) } catch (_: Exception) { null }
        val signers = try {
            info.signingInfo.apkContentsSigners?.map { it.toByteArray() }.orEmpty()
        } catch (_: Exception) {
            emptyList()
        }
        return CallerIdentity.resolve(info.packageName, context.packageName, info.isOriginPopulated(), origin, signers)
    }
}
