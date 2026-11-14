package app.gitpass.android.pairing

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Real bridge, both roles, on the loopback interface of the device. */
@RunWith(AndroidJUnit4::class)
class PairingLoopbackTest {
    @Test fun loopbackGrantKeepsSecretsOffTheWire() {
        val root = InstrumentationRegistry.getInstrumentation().targetContext.cacheDir
        val hostDir = File(root, "pair-host-${UUID.randomUUID()}")
        val joinDir = File(root, "pair-join-${UUID.randomUUID()}").apply { mkdirs() }
        PairingHostPeer(hostDir).use { peer ->
            assertTrue(peer.qr.startsWith("gitpass-pair:2:qr:"))
            assertFalse("qr has a secret", peer.qr.contains(PAIRING_TEST_TOKEN) || peer.qr.contains(PAIRING_TEST_SECRET) || peer.qr.contains("master"))
            val client = LanPairingClient(joinDir)
            try {
                val connected = client.connectUntilCode(peer.qr)
                assertEquals(6, connected.code.length)
                assertTrue(connected.code.all { it.isDigit() })
                peer.confirm(connected.code)
                val paired = client.receiveGrant()
                try {
                    assertTrue("grant token mismatch", paired.syncToken() == PAIRING_TEST_TOKEN)
                    assertEquals("fake-owner", paired.repoOwner())
                    assertEquals("fake-vault", paired.repoName())
                    assertEquals("main", paired.branch())
                    assertEquals("fine_grained_pat", paired.tokenKind())
                } finally {
                    paired.close()
                }
            } finally {
                client.close()
            }
            val wire = peer.wireText()
            assertFalse(
                "wire has a secret",
                wire.contains(PAIRING_TEST_TOKEN) || wire.contains(PAIRING_TEST_SECRET) || wire.contains("master_password") || wire.contains("FAKE-master"),
            )
            assertNull(peer.failure)
        }
    }
}
