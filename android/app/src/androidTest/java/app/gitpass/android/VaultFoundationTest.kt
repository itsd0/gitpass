package app.gitpass.android

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import app.gitpass.android.session.FfiVaultGateway
import app.gitpass.android.session.ItemSearch
import app.gitpass.android.session.VaultLockedException
import app.gitpass.android.session.VaultSession
import app.gitpass.android.session.detailRows
import app.gitpass.core.FfiField
import app.gitpass.core.FfiSection
import app.gitpass.core.GitpassException
import app.gitpass.core.ItemInput
import app.gitpass.core.createAccount
import app.gitpass.core.openVault
import java.io.File
import java.util.Collections
import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Real gitpass-core vault in the app cache. Production no-backup storage
 * is not used. Values here are fake.
 */
@RunWith(AndroidJUnit4::class)
class VaultFoundationTest {
    @Test
    fun unlockSearchDetailLockTamperAndRaces() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "vault-foundation-${UUID.randomUUID()}")
        val tamperedKeyset = File(context.cacheDir, "vault-tamper-key-${UUID.randomUUID()}")
        val tamperedItem = File(context.cacheDir, "vault-tamper-item-${UUID.randomUUID()}")
        try {
            val password = "correct horse"
            val secret = createAccount(root.absolutePath, password)
            assertTrue(secret.startsWith("A3-"))
            val seeded = openVault(root.absolutePath, password, secret)
            val vaultId = seeded.createVault("Personal", null)
            val kept = seeded.createItem(
                vaultId,
                ItemInput(
                    title = "Example",
                    username = "alice",
                    password = "s3cret!",
                    url = "https://example.invalid",
                    notes = "fake-note",
                    totp = null,
                    category = "login",
                    tags = listOf("fake-tag"),
                    favorite = true,
                    sections = listOf(
                        FfiSection(
                            id = "extra",
                            title = "Additional",
                            fields = listOf(
                                FfiField("custom-1", "Recovery hint", "text", "fake-hint"),
                                FfiField("pin", "PIN", "concealed", "4242"),
                            ),
                        ),
                    ),
                ),
            )
            val gone = seeded.createItem(
                vaultId,
                ItemInput("Gone", null, null, null, null, null, "login", emptyList(), false, emptyList()),
            )
            seeded.deleteItem(vaultId, gone)
            seeded.close()

            root.copyRecursively(tamperedKeyset)
            root.copyRecursively(tamperedItem)

            var now = 10_000L
            val session = VaultSession(FfiVaultGateway(), nowMs = { now }, autoLockMs = 1_000)
            session.unlock(root.absolutePath, password, secret)
            val listed = ItemSearch.visible(session.snapshot().items, "")
            assertEquals(listOf("Example"), listed.map { it.title })
            assertFalse(listed.joinToString { "${it.title}|${it.subtitle}|${it.url}" }.contains("s3cret"))
            session.setQuery("fake-tag")
            assertEquals(1, session.snapshot().items.size)
            session.setQuery("Gone")
            assertTrue(session.snapshot().items.isEmpty())
            session.setQuery("")
            val detail = session.openDetail(kept)
            assertEquals("s3cret!", detail.password)
            val rows = detailRows(detail)
            assertTrue(rows.any { it.label == "Recovery hint" && it.value == "fake-hint" && !it.secret })
            assertTrue(rows.any { it.label == "PIN" && it.secret && it.value == "4242" })
            now = 11_000
            session.enforceInactivity()
            assertFalse(session.isUnlocked())

            session.unlock(root.absolutePath, password, secret)
            val errors = Collections.synchronizedList(mutableListOf<Throwable>())
            val threads = (1..4).map {
                Thread {
                    repeat(8) {
                        try {
                            session.reload()
                        } catch (_: VaultLockedException) {
                        } catch (error: Throwable) {
                            errors.add(error)
                        }
                    }
                }
            }
            threads.forEach { it.start() }
            Thread.sleep(50)
            session.lock()
            threads.forEach { it.join(10_000) }
            assertTrue(errors.toString(), errors.isEmpty())
            assertFalse(session.isUnlocked())

            try {
                session.unlock(root.absolutePath, "wrong password", secret)
                throw AssertionError("wrong password must not unlock")
            } catch (error: GitpassException.Generic) {
                assertTrue(error.v1.contains("unlock failed"))
                assertFalse(error.v1.contains("s3cret"))
            }
            assertFalse(session.isUnlocked())

            tamperEnvelope(File(tamperedKeyset, "keyset.enc"))
            val rejected = VaultSession(FfiVaultGateway(), autoLockMs = 60_000)
            try {
                rejected.unlock(tamperedKeyset.absolutePath, password, secret)
                throw AssertionError("tampered keyset must not unlock")
            } catch (error: GitpassException.Generic) {
                assertTrue(error.v1.contains("tampered") || error.v1.contains("unlock failed"))
                assertFalse(error.v1.contains("s3cret"))
            }
            assertFalse(rejected.isUnlocked())

            val itemSession = VaultSession(FfiVaultGateway(), autoLockMs = 60_000)
            itemSession.unlock(tamperedItem.absolutePath, password, secret)
            val itemId = itemSession.snapshot().items.single { it.title == "Example" }.id
            tamperEnvelope(File(tamperedItem, "vaults/$vaultId/items/$itemId.enc"))
            try {
                itemSession.openDetail(itemId)
                throw AssertionError("tampered item must not decrypt")
            } catch (error: GitpassException.Generic) {
                assertFalse(error.v1.contains("s3cret"))
                assertNotNull(error.v1)
            }
            assertTrue(itemSession.isUnlocked())
            itemSession.lock()
            assertFalse(itemSession.isUnlocked())
        } finally {
            root.deleteRecursively()
            tamperedKeyset.deleteRecursively()
            tamperedItem.deleteRecursively()
        }
    }

    private fun tamperEnvelope(file: File) {
        val text = file.readText()
        val marker = listOf("\"ct\": \"", "\"ct\":\"").firstOrNull { text.contains(it) }
            ?: throw AssertionError("encrypted envelope was expected")
        val start = text.indexOf(marker)
        val at = start + marker.length + 8
        val chars = text.toCharArray()
        chars[at] = if (chars[at] == 'A') 'B' else 'A'
        file.writeText(String(chars))
    }
}
