package app.gitpass.android.session

import java.io.File
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
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
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher

class VaultSessionTest {
    @Test fun importSelectionGateRejectsStaleAndUnlockedEpochsAndLockCancelsImmediately() {
        var cancellations = 0
        val session = VaultSession(readyGateway(), nowMs = { 1000L }, onLock = { cancellations++ })
        val epoch = session.state.value.epoch
        assertEquals("FAKE permitted", session.atLockedEpoch(epoch) { "FAKE permitted" })
        session.onHostBackground()
        assertEquals(1, cancellations)
        assertThrows(UnlockCancelledException::class.java) { session.atLockedEpoch(epoch) { error("must not activate") } }
        val lockedEpoch = session.state.value.epoch
        session.unlock("root", "right-password", "A3-RIGHT")
        assertThrows(UnlockCancelledException::class.java) { session.atLockedEpoch(lockedEpoch) { error("must not activate") } }
        session.lock()
        assertEquals(2, cancellations)
    }

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

    @Test
    fun everyLockPathPublishesALockedStateWithoutDetail() {
        var now = 0L
        val gateway = readyGateway()
        val session = VaultSession(gateway, nowMs = { now })
        val seen = mutableListOf<VaultSnapshot>()

        fun openRevealed(): Long {
            session.unlock("root", "right-password", "A3-RIGHT")
            session.setQuery("ali")
            session.openDetail("item-1")
            val open = session.state.value
            assertTrue(open.unlocked)
            assertEquals("s3cret!", open.detail?.password)
            assertEquals("ali", open.query)
            return open.epoch
        }

        fun assertLockedFrom(epoch: Long) {
            val locked = session.state.value
            seen += locked
            assertFalse(locked.unlocked)
            assertNull(locked.detail)
            assertTrue(locked.items.isEmpty())
            assertTrue(locked.vaults.isEmpty())
            assertEquals("", locked.query)
            assertTrue("lock must start a new UI epoch", locked.epoch > epoch)
        }

        // Manual lock: no snapshot() poll, no ticker.
        var epoch = openRevealed()
        session.lock()
        assertLockedFrom(epoch)

        // Background lock.
        epoch = openRevealed()
        session.onHostBackground()
        assertLockedFrom(epoch)

        // Inactivity lock after exactly five minutes of monotonic time.
        epoch = openRevealed()
        now += VaultSession.DEFAULT_AUTO_LOCK_MS
        session.enforceInactivity()
        assertLockedFrom(epoch)

        assertEquals(3, seen.map { it.epoch }.distinct().size)
        assertEquals(0, gateway.live.get())
    }

    @Test
    fun fiveMinuteInactivityExpiryOnTheMonotonicClock() {
        var monotonic = 50_000L
        val gateway = readyGateway()
        val session = VaultSession(gateway, nowMs = { monotonic })
        assertEquals(5 * 60 * 1000L, VaultSession.DEFAULT_AUTO_LOCK_MS)
        assertNull(session.millisUntilAutoLock())
        session.unlock("root", "right-password", "A3-RIGHT")
        assertEquals(VaultSession.DEFAULT_AUTO_LOCK_MS, session.millisUntilAutoLock())

        monotonic += VaultSession.DEFAULT_AUTO_LOCK_MS - 1
        session.enforceInactivity()
        assertTrue(session.state.value.unlocked)
        assertEquals(1L, session.millisUntilAutoLock())

        // Activity pushes the deadline out by a full five minutes.
        session.noteActivity()
        monotonic += VaultSession.DEFAULT_AUTO_LOCK_MS - 1
        session.enforceInactivity()
        assertTrue(session.isUnlocked())
        monotonic += 1
        session.enforceInactivity()
        assertFalse(session.isUnlocked())
        assertFalse(session.state.value.unlocked)
        assertNull(session.millisUntilAutoLock())
    }

    @Test
    fun wallClockChangesCannotDelayTheLock() {
        // The session's only time source is the injected monotonic clock.
        // Simulate the device wall clock being set back by a day mid-session:
        // it is not an input, so the lock still fires at 5 monotonic minutes.
        var monotonic = 0L
        var wall = 1_800_000_000_000L
        val session = VaultSession(readyGateway(), nowMs = { monotonic })
        session.unlock("root", "right-password", "A3-RIGHT")
        wall -= 24 * 60 * 60 * 1000L
        monotonic += VaultSession.DEFAULT_AUTO_LOCK_MS
        session.enforceInactivity()
        assertFalse(session.isUnlocked())
        assertTrue(wall > 0)

        // And the production source does not read the wall clock at all.
        val source = File("src/main/java/app/gitpass/android/session/VaultSession.kt").readText()
        assertFalse(source.contains("currentTimeMillis"))
        assertTrue(source.contains("SystemClock.elapsedRealtime()"))
    }

    @Test
    fun createAccountHandsOffTheKeyBeforeUnlocking() {
        val gateway = MemoryGateway()
        val session = session(gateway)
        val handed = AtomicReference<String>()
        gateway.beforeOpen = { assertEquals("A3-NEW", handed.get()) }
        val secret = session.createAccount("root", "right-password") { handed.set(it) }
        assertEquals("A3-NEW", secret)
        assertTrue(session.state.value.unlocked)
    }

    private fun session(gateway: MemoryGateway) = VaultSession(gateway, nowMs = { 0L }, autoLockMs = 60_000)

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
    var beforeOpen: () -> Unit = {}
    var createEntered = CountDownLatch(0)
    var createRelease = CountDownLatch(0)
    val openEntered = CountDownLatch(1)
    val openRelease = CountDownLatch(1)
    val listEntered = CountDownLatch(1)
    val listRelease = CountDownLatch(1)

    override fun vaultExists(root: String): Boolean = exists

    override fun createAccount(root: String, password: String): String {
        creates.incrementAndGet()
        exists = true
        createEntered.countDown()
        check(createRelease.await(5, TimeUnit.SECONDS))
        return "A3-NEW"
    }

    override fun open(root: String, password: String, secretKey: String): OpenVault {
        beforeOpen()
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

class AccountSetupTest {
    private val executor = Executors.newSingleThreadExecutor()
    private val scope = CoroutineScope(SupervisorJob() + executor.asCoroutineDispatcher())

    @org.junit.After
    fun tearDown() {
        executor.shutdownNow()
    }

    @Test
    fun keyIsPendingAsSoonAsTheAccountExistsAndSurvivesTheCallerGoingAway() {
        val gateway = MemoryGateway()
        gateway.createEntered = CountDownLatch(1)
        gateway.createRelease = CountDownLatch(1)
        val pending = PendingSecret()
        val dir = Files.createTempDirectory("gitpass-setup").toFile()
        val marker = File(dir, "vault.setup-unconfirmed")
        val session = VaultSession(gateway, nowMs = { 0L })
        val setup = AccountSetup(session, pending, marker, scope) { gateway.exists }
        try {
            gateway.beforeOpen = { check(pending.get() == "A3-NEW") { "key not pending before unlock" } }
            assertTrue(setup.start("root", "right-password"))
            assertFalse("second start while creating", setup.start("root", "right-password"))
            assertTrue(gateway.createEntered.await(5, TimeUnit.SECONDS))
            assertTrue("marker is written before the account exists", marker.isFile)
            assertEquals("", marker.readText())
            // The UI that started creation is gone now; nothing it owned matters.
            gateway.createRelease.countDown()
            awaitState(setup) { it == AccountSetup.State.Idle }
            assertEquals("A3-NEW", pending.get())
            assertTrue(pending.present.value)
            assertTrue(session.isUnlocked())
            assertFalse(setup.isInterrupted("root"))
            assertFalse(pending.toString().contains("A3-NEW"))

            setup.confirmSaved()
            assertNull(pending.get())
            assertFalse(pending.present.value)
            assertFalse(marker.exists())
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun unlockFailureAfterCreationStillHandsOffTheKey() {
        val gateway = MemoryGateway()
        gateway.failOpen = true
        val pending = PendingSecret()
        val dir = Files.createTempDirectory("gitpass-setup").toFile()
        val marker = File(dir, "vault.setup-unconfirmed")
        val setup = AccountSetup(VaultSession(gateway, nowMs = { 0L }), pending, marker, scope) { gateway.exists }
        try {
            assertTrue(setup.start("root", "right-password"))
            val failed = awaitState(setup) { it is AccountSetup.State.Failed } as AccountSetup.State.Failed
            assertEquals("A3-NEW", pending.get())
            assertFalse(failed.error is AccountCreatedButLocked)
            assertFalse(failed.toString().contains("A3-NEW"))
            assertTrue(marker.isFile)
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun processDeathBeforeConfirmationIsReportedNotHidden() {
        val gateway = MemoryGateway()
        val dir = Files.createTempDirectory("gitpass-setup").toFile()
        val marker = File(dir, "vault.setup-unconfirmed")
        try {
            val first = AccountSetup(VaultSession(gateway, nowMs = { 0L }), PendingSecret(), marker, scope) { gateway.exists }
            assertTrue(first.start("root", "right-password"))
            awaitState(first) { it == AccountSetup.State.Idle }

            // New process: memory is empty, marker and vault are on disk.
            val pending = PendingSecret()
            val afterDeath = AccountSetup(VaultSession(gateway, nowMs = { 0L }), pending, marker, scope) { gateway.exists }
            afterDeath.discardStaleMarker("root")
            assertNull(pending.get())
            assertTrue(afterDeath.isInterrupted("root"))

            // Unlocking with a key the owner wrote down clears the warning.
            afterDeath.noteUnlocked()
            assertFalse(afterDeath.isInterrupted("root"))
            assertFalse(marker.exists())
        } finally {
            dir.deleteRecursively()
        }
    }

    @Test
    fun staleMarkerWithoutAVaultIsDropped() {
        val gateway = MemoryGateway()
        val dir = Files.createTempDirectory("gitpass-setup").toFile()
        val marker = File(dir, "vault.setup-unconfirmed").apply { writeText("") }
        try {
            val setup = AccountSetup(VaultSession(gateway, nowMs = { 0L }), PendingSecret(), marker, scope) { gateway.exists }
            setup.discardStaleMarker("root")
            assertFalse(marker.exists())
            assertFalse(setup.isInterrupted("root"))
        } finally {
            dir.deleteRecursively()
        }
    }

    private fun awaitState(setup: AccountSetup, done: (AccountSetup.State) -> Boolean): AccountSetup.State {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (System.nanoTime() < deadline) {
            val current = setup.state.value
            if (done(current)) return current
            Thread.sleep(10)
        }
        throw AssertionError("setup state stayed ${setup.state.value}")
    }
}
