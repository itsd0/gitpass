package app.gitpass.android.session

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class VaultSessionTest {
    @Test
    fun wrongCredentialsStayLocked() {
        val gateway = readyGateway()
        val session = session(gateway)
        val error = assertThrows(IllegalStateException::class.java) {
            session.unlock("root", "nope-password", "A3-RIGHT")
        }
        assertTrue(error.message!!.contains("tampered"))
        assertFalse(session.isUnlocked())
        assertFalse(session.snapshot().unlocked)
        assertEquals(0, gateway.live.get())
    }

    @Test
    fun shortPasswordDoesNotCreateAnAccount() {
        val gateway = MemoryGateway()
        val session = session(gateway)
        assertThrows(VaultSessionException::class.java) {
            session.createAccount("root", "short")
        }
        assertEquals(0, gateway.creates.get())
        assertFalse(gateway.vaultExists("root"))
    }

    @Test
    fun backgroundDuringUnlockClosesTheNewSession() {
        val gateway = readyGateway()
        gateway.blockOpen = true
        val session = session(gateway)
        val failure = AtomicReference<Throwable>()
        val worker = Thread {
            try {
                session.unlock("root", "right-password", "A3-RIGHT")
            } catch (error: Throwable) {
                failure.set(error)
            }
        }
        worker.start()
        assertTrue(gateway.openEntered.await(5, TimeUnit.SECONDS))
        session.onHostBackground()
        gateway.openRelease.countDown()
        worker.join(5_000)
        assertFalse(worker.isAlive)
        assertFalse(session.isUnlocked())
        assertEquals(0, gateway.live.get())
        assertTrue(failure.get() is UnlockCancelledException)
        assertNull(session.snapshot().detail)
        assertEquals("", session.snapshot().query)
    }

    @Test
    fun lockWaitsForInFlightReadThenRejectsNewReads() {
        val gateway = readyGateway()
        val session = session(gateway)
        session.unlock("root", "right-password", "A3-RIGHT")
        gateway.armBlock = true
        val readerError = AtomicReference<Throwable>()
        val reader = Thread {
            try {
                session.reload()
            } catch (error: Throwable) {
                readerError.set(error)
            }
        }
        reader.start()
        assertTrue(gateway.listEntered.await(5, TimeUnit.SECONDS))
        val locker = Thread { session.lock() }
        locker.start()
        Thread.sleep(150)
        assertTrue("lock returned while a read still held the session", locker.isAlive)
        gateway.listRelease.countDown()
        reader.join(5_000)
        locker.join(5_000)
        assertFalse(reader.isAlive)
        assertFalse(locker.isAlive)
        assertNull(readerError.get())
        assertFalse(session.isUnlocked())
        assertEquals(0, gateway.live.get())
        assertThrows(VaultLockedException::class.java) { session.reload() }
        assertNull(session.snapshot().detail)
    }

    @Test
    fun inactivityAndBackgroundClearDetail() {
        var now = 1_000L
        val gateway = readyGateway()
        val session = VaultSession(gateway, nowMs = { now }, autoLockMs = 1_000)
        session.unlock("root", "right-password", "A3-RIGHT")
        session.setQuery("ali")
        val detail = session.openDetail("item-1")
        assertEquals("s3cret!", detail.password)
        now = 1_999
        session.enforceInactivity()
        assertTrue(session.isUnlocked())
        assertEquals("Example", session.snapshot().detail?.title)
        now = 2_000
        session.enforceInactivity()
        assertFalse(session.isUnlocked())
        assertNull(session.snapshot().detail)
        assertTrue(session.snapshot().items.isEmpty())

        session.unlock("root", "right-password", "A3-RIGHT")
        session.openDetail("item-1")
        session.onHostBackground()
        assertFalse(session.isUnlocked())
        assertNull(session.snapshot().detail)
    }

    @Test
    fun searchHidesDeletedAndArchivedAndDoesNotSeeSecrets() {
        val visible = ItemSearch.visible(
            listOf(
                row("1", "Example", deleted = false, archived = false),
                row("2", "Gone", deleted = true, archived = false),
                row("3", "Old", deleted = false, archived = true),
            ),
            "e",
        )
        assertEquals(listOf("1"), visible.map { it.id })
        val dumped = visible.joinToString { "${it.title}|${it.subtitle}|${it.url}" }
        assertFalse(dumped.contains("s3cret"))
    }

    @Test
    fun detailKeepsSectionFieldsWithoutDuplicatingLogin() {
        val item = ItemView(
            id = "item-1",
            vaultId = "v1",
            title = "Example",
            username = "alice",
            password = "s3cret!",
            url = "https://example.invalid",
            notes = "note",
            totp = "JBSWY3DPEHPK3PXP",
            category = "login",
            tags = listOf("fake"),
            favorite = false,
            archived = false,
            deleted = false,
            updatedAt = "1",
            sections = listOf(
                SectionView(
                    id = "login",
                    title = null,
                    fields = listOf(
                        FieldView("username", "username", "text", "alice"),
                        FieldView("password", "password", "concealed", "s3cret!"),
                    ),
                ),
                SectionView(
                    id = "extra",
                    title = "Additional",
                    fields = listOf(
                        FieldView("custom-1", "Recovery hint", "text", "fake-hint"),
                        FieldView("pin", "PIN", "concealed", "4242"),
                        FieldView("vendor", "Vendor", "custom-widget", "acme"),
                    ),
                ),
            ),
        )
        val rows = detailRows(item)
        assertEquals(1, rows.count { it.label == "password" })
        assertTrue(rows.any { it.label == "Recovery hint" && !it.secret && it.value == "fake-hint" })
        assertTrue(rows.any { it.label == "PIN" && it.secret })
        assertTrue(rows.any { it.label == "Vendor" && !it.secret && it.value == "acme" })
        assertTrue(rows.any { it.key == "totp" && it.secret })
    }

    @Test
    fun createdAccountFailureDoesNotPutTheSecretInTheMessage() {
        val gateway = MemoryGateway()
        gateway.failOpen = true
        val session = session(gateway)
        val error = assertThrows(AccountCreatedButLocked::class.java) {
            session.createAccount("root", "right-password")
        }
        assertEquals("A3-NEW", error.secretKey)
        assertFalse(error.message!!.contains("A3-NEW"))
        assertFalse(error.toString().contains("A3-NEW"))
        assertFalse(session.isUnlocked())
    }

    @Test
    fun pendingSecretToStringHidesTheKey() {
        val pending = PendingSecret()
        pending.set("A3-SECRET-VALUE")
        assertEquals("A3-SECRET-VALUE", pending.get())
        assertFalse(pending.toString().contains("A3-SECRET-VALUE"))
        pending.clear()
        assertNull(pending.get())
    }

    private fun session(gateway: MemoryGateway) = VaultSession(gateway, autoLockMs = 60_000)

    private fun readyGateway(): MemoryGateway = MemoryGateway().also { it.exists = true }

    private fun row(id: String, title: String, deleted: Boolean, archived: Boolean) = ItemRow(
        id = id,
        vaultId = "v1",
        title = title,
        subtitle = "alice",
        url = "https://example.invalid",
        category = "login",
        tags = listOf("fake"),
        favorite = false,
        archived = archived,
        deleted = deleted,
        hasTotp = false,
    )
}

private class MemoryGateway : VaultGateway {
    val creates = AtomicInteger()
    val live = AtomicInteger()
    var blockOpen = false
    var armBlock = false
    var failOpen = false
    var exists = false
    val openEntered = CountDownLatch(1)
    val openRelease = CountDownLatch(1)
    val listEntered = CountDownLatch(1)
    val listRelease = CountDownLatch(1)

    override fun vaultExists(root: String): Boolean = exists

    override fun createAccount(root: String, password: String): String {
        creates.incrementAndGet()
        exists = true
        return "A3-NEW"
    }

    override fun open(root: String, password: String, secretKey: String): OpenVault {
        if (blockOpen) {
            openEntered.countDown()
            check(openRelease.await(5, TimeUnit.SECONDS))
        }
        if (failOpen) throw IllegalStateException("unlock failed: tampered")
        if (password != "right-password" || (secretKey != "A3-RIGHT" && secretKey != "A3-NEW")) {
            throw IllegalStateException("unlock failed: wrong master password or Secret Key (or format.json/keyset.enc tampered)")
        }
        exists = true
        return MemoryVault(this)
    }
}

private class MemoryVault(private val gateway: MemoryGateway) : OpenVault {
    init {
        gateway.live.incrementAndGet()
    }

    override fun listVaults(): List<VaultSummary> = listOf(VaultSummary("v1", "Personal", null, 1))

    override fun listItems(vaultId: String): List<ItemRow> {
        if (gateway.armBlock) {
            gateway.listEntered.countDown()
            check(gateway.listRelease.await(5, TimeUnit.SECONDS))
        }
        return listOf(
            ItemRow(
                id = "item-1",
                vaultId = vaultId,
                title = "Example",
                subtitle = "alice",
                url = "https://example.invalid",
                category = "login",
                tags = listOf("fake"),
                favorite = false,
                archived = false,
                deleted = false,
                hasTotp = false,
            ),
        )
    }

    override fun getItem(vaultId: String, itemId: String): ItemView = ItemView(
        id = itemId,
        vaultId = vaultId,
        title = "Example",
        username = "alice",
        password = "s3cret!",
        url = "https://example.invalid",
        notes = null,
        totp = null,
        category = "login",
        tags = listOf("fake"),
        favorite = false,
        archived = false,
        deleted = false,
        updatedAt = "1",
        sections = emptyList(),
    )

    override fun createVault(name: String): String = "v1"

    override fun close() {
        gateway.live.decrementAndGet()
    }
}
