package app.gitpass.android

import android.content.res.AssetManager
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.core.openVault
import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Shared encrypted fixtures (issue #451): every committed fixture opens
 * through the UniFFI bridge and decrypts to its committed expected.json.
 *
 * The fixtures ship as androidTest assets (see build.gradle.kts) and are
 * copied to the app cache before opening — never opened in place, because
 * a normal unlock writes device state and may re-wrap the keyset.
 * Generator history is not exposed by the bridge, so that section is
 * covered by the core test only. All values here are fake.
 */
@RunWith(AndroidJUnit4::class)
class SharedFixturesTest {
    @Test
    fun sharedFixturesMatchExpected() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val assets = instrumentation.context.assets
        val cache = instrumentation.targetContext.cacheDir
        for (version in listOf(2, 3, 4, 5)) {
            val dir = File(cache, "shared-fixture-v$version-${System.nanoTime()}")
            try {
                copyAssetDir(assets, "v$version", dir)
                val expected = JSONObject(dir.resolve("expected.json").readText())
                // Version is read BEFORE opening: a normal unlock re-wraps
                // the keyset to the production KDF and bumps format.json.
                val format = JSONObject(dir.resolve("format.json").readText())
                assertEquals(version, format.getInt("version"))
                val secret = dir.resolve("secret.key.txt").readText().trim()
                val vault = openVault(dir.absolutePath, PASSWORD, secret)
                try {
                    val vaults = JSONArray()
                    for (info in vault.listVaults()) {
                        val items = JSONArray()
                        for (summary in vault.listItems(info.id)) {
                            val record = JSONObject(vault.recordJson(info.id, summary.id))
                            stripRecord(record)
                            items.put(record)
                        }
                        vaults.put(
                            vaultJson(
                                info.name,
                                info.description,
                                info.icon,
                                info.color,
                                sortBy(items, "title"),
                            ),
                        )
                    }
                    val got = JSONObject()
                    got.put("format_version", format.getInt("version"))
                    got.put("vaults", sortBy(vaults, "name"))
                    val want = JSONObject(expected.toString())
                    want.remove("generator_history")
                    assertEquals(
                        "v$version decrypted content via the bridge",
                        canonical(want),
                        canonical(got),
                    )
                } finally {
                    vault.close()
                }
            } finally {
                dir.deleteRecursively()
            }
        }
    }

    private fun vaultJson(
        name: String,
        description: String?,
        icon: String?,
        color: String?,
        items: JSONArray,
    ): JSONObject {
        val v = JSONObject()
        v.put("name", name)
        v.put("description", description ?: JSONObject.NULL)
        if (icon != null) v.put("icon", icon)
        if (color != null) v.put("color", color)
        v.put("items", items)
        return v
    }

    private fun sortBy(array: JSONArray, key: String): JSONArray {
        val list = (0 until array.length())
            .map { array.getJSONObject(it) }
            .sortedBy { it.optString(key) }
        val out = JSONArray()
        list.forEach { out.put(it) }
        return out
    }

    /** Drop every volatile field from one serialized ItemRecord, mirroring
     * `gitpass_core::test_fixtures::semantic_record`: wall-clock
     * timestamps, merge clocks, random attachment ids; `deleted_at`
     * becomes an explicit `deleted` boolean. */
    private fun stripRecord(record: JSONObject) {
        stripVolatile(record)
        if (!record.has("deleted")) {
            record.put("deleted", false)
        }
    }

    private fun stripVolatile(obj: JSONObject) {
        for (key in listOf("updated_at", "created_at", "clocks", "xclocks", "replaced_at")) {
            obj.remove(key)
        }
        if (obj.has("deleted_at")) {
            val deleted = !obj.isNull("deleted_at")
            obj.remove("deleted_at")
            obj.put("deleted", deleted)
        }
        if (obj.has("attachments")) {
            val atts = obj.getJSONArray("attachments")
            for (i in 0 until atts.length()) {
                atts.getJSONObject(i).remove("id")
            }
        }
        for (key in obj.keys()) {
            when (val v = obj.opt(key)) {
                is JSONObject -> stripVolatile(v)
                is JSONArray -> for (i in 0 until v.length()) {
                    val e = v.opt(i)
                    if (e is JSONObject) stripVolatile(e)
                }
            }
        }
    }

    /** Canonical JSON: object keys sorted, arrays in order. */
    private fun canonical(value: Any?): String {
        return when (value) {
            null, JSONObject.NULL -> "null"
            is JSONObject -> {
                val keys = value.keys().asSequence().toList().sorted()
                keys.joinToString(",", "{", "}") { k -> "${JSONObject.quote(k)}:${canonical(value.opt(k))}" }
            }
            is JSONArray -> (0 until value.length()).joinToString(",", "[", "]") { canonical(value.opt(it)) }
            is String -> JSONObject.quote(value)
            is Number, is Boolean -> value.toString()
            else -> JSONObject.quote(value.toString())
        }
    }

    private fun copyAssetDir(assets: AssetManager, path: String, dest: File) {
        val children = assets.list(path)
        if (children.isNullOrEmpty()) {
            dest.parentFile?.mkdirs()
            assets.open(path).use { input ->
                dest.outputStream().use { input.copyTo(it) }
            }
        } else {
            dest.mkdirs()
            for (child in children) {
                copyAssetDir(assets, "$path/$child", File(dest, child))
            }
        }
    }

    companion object {
        /** Test-only master password for the shared fixtures. */
        const val PASSWORD = "fixture-password-451"
    }
}
