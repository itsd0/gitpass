package app.gitpass.android.session

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BridgeCompatTest {
    @Test fun acceptsOnlyTheExpectedApiLevelAndFormat() {
        assertTrue(BridgeCompat.isCompatible(BridgeCompat.EXPECTED_API_LEVEL, 2u, BridgeCompat.EXPECTED_FORMAT))
        assertFalse(BridgeCompat.isCompatible(BridgeCompat.EXPECTED_API_LEVEL + 1u, 2u, BridgeCompat.EXPECTED_FORMAT))
        assertFalse(BridgeCompat.isCompatible(0u, 2u, BridgeCompat.EXPECTED_FORMAT))
        assertFalse(BridgeCompat.isCompatible(BridgeCompat.EXPECTED_API_LEVEL, 2u, BridgeCompat.EXPECTED_FORMAT + 1u))
        assertFalse(BridgeCompat.isCompatible(BridgeCompat.EXPECTED_API_LEVEL, 9u, BridgeCompat.EXPECTED_FORMAT))
    }
}
