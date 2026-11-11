package app.gitpass.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.core.isAllowedPath
import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Shared allowed-path table (issue #452): the bridge's isAllowedPath, which
 * is core's single rule, accepts and rejects exactly the table's paths. The
 * table ships with the shared fixtures as androidTest assets. Test data only.
 */
@RunWith(AndroidJUnit4::class)
class SharedAllowedPathsTest {
    @Test
    fun sharedAllowedPathTableMatchesBridge() {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val table = JSONObject(assets.open("allowed_paths.json").use { it.readBytes().decodeToString() })
        val allowed = table.getJSONArray("allowed")
        val rejected = table.getJSONArray("rejected")
        assertTrue(allowed.length() >= 10 && rejected.length() >= 20)
        for (i in 0 until allowed.length()) {
            val p = allowed.getString(i)
            assertTrue("must allow ${JSONObject.quote(p)}", isAllowedPath(p))
        }
        for (i in 0 until rejected.length()) {
            val p = rejected.getString(i)
            assertFalse("must reject ${JSONObject.quote(p)}", isAllowedPath(p))
        }
    }
}
