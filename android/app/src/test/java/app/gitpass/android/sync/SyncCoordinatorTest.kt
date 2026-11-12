package app.gitpass.android.sync

import app.gitpass.android.session.*
import java.io.File
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.*
import org.junit.Test

/** Runs tasks only when the test advances time; deterministic, single-threaded. */
class ManualScheduler : SyncScheduler {
    private class Entry(val due: Long, val task: () -> Unit) { var cancelled = false }
    private val entries = mutableListOf<Entry>()
    var now = 0L
        private set
    override fun schedule(delayMs: Long, task: () -> Unit): ScheduledTask {
        val e = Entry(now + delayMs, task)
        synchronized(entries) { entries += e }
        return ScheduledTask { e.cancelled = true }
    }
    fun advance(ms: Long) {
        val target = now + ms
        while (true) {
            val next = synchronized(entries) { entries.filter { !it.cancelled && it.due <= target }.minByOrNull { it.due }?.also { entries -= it } } ?: break
            now = next.due
            next.task()
        }
        now = target
    }
    fun pending() = synchronized(entries) { entries.count { !it.cancelled } }
}

/** Fake account: in-memory items; syncGitHub is scripted by the test. */
class SyncFakeGateway : VaultGateway, OpenVault {
    val syncs = AtomicInteger()
    var outcome = SyncOutcome(pushed = true, localChanged = false, conflicts = emptyList(), mergedItems = 0, localOnlyItems = 0)
    var failure: Exception? = null
    var onSync: (SyncCancel) -> Unit = {}
    var lastToken: String? = null
    val titles = linkedMapOf("i1" to "One")
    override fun vaultExists(root: String) = true
    override fun createAccount(root: String, password: String) = "A3-FAKE"
    override fun open(root: String, password: String, secretKey: String): OpenVault = this
    override fun listVaults() = listOf(VaultSummary("v1", "Fake", null, titles.size.toLong()))
    override fun listItems(vaultId: String) = titles.map { (id, t) -> ItemRow(id, "v1", t, "", null, "login", emptyList(), false, false, false, false) }
    override fun getItem(vaultId: String, itemId: String) = ItemView(itemId, "v1", titles.getValue(itemId), null, null, null, null, null, "login", emptyList(), false, false, false, "", emptyList())
    override fun createVault(name: String) = "v1"
    override fun createItem(vaultId: String, input: app.gitpass.core.ItemInput): String { titles["i${titles.size + 1}"] = input.title; return "i${titles.size}" }
    override fun updateItem(vaultId: String, itemId: String, input: app.gitpass.core.ItemInput) { titles[itemId] = input.title }
    override fun syncGitHub(target: SyncTarget, token: String, cancel: SyncCancel): SyncOutcome {
        syncs.incrementAndGet()
        lastToken = token
        onSync(cancel)
        failure?.let { throw it }
        return outcome
    }
    override fun close() {}
}

class SyncCoordinatorTest {
    private val token = "github_pat_FAKE_TOKEN_0123456789"

    private class Rig(configured: Boolean = true, var online: Boolean = true) {
        val gateway = SyncFakeGateway()
        val scheduler = ManualScheduler()
        val session = VaultSession(gateway, nowMs = { 0L }, autoLockMs = 0L)
        val settings = SyncSettingsStore(File(Files.createTempDirectory("sync-coord").toFile(), "a.sync.json"), FakeCipher())
        val coordinator: SyncCoordinator
        init {
            if (configured) settings.save(SyncTarget("FAKE-owner", "fake-vault", "main"), "github_pat_FAKE_TOKEN_0123456789")
            coordinator = SyncCoordinator(session, settings, scheduler, online = { online }, wallClock = { 1_700_000_000_000L })
            session.events = coordinator
        }
        fun unlock() = session.unlock("root", "FAKE-password", "A3-FAKE")
    }

    @Test fun unlockSyncsOnceAndRecordsSuccess() {
        val rig = Rig()
        assertEquals(SyncCoordinator.Phase.Idle, rig.coordinator.status.value.phase)
        rig.unlock()
        rig.scheduler.advance(0)
        assertEquals(1, rig.gateway.syncs.get())
        assertEquals(token, rig.gateway.lastToken)
        val s = rig.coordinator.status.value
        assertEquals(SyncCoordinator.Phase.Idle, s.phase)
        assertEquals(1_700_000_000_000L, s.lastSuccessMs)
        assertEquals(1_700_000_000_000L, rig.settings.lastSuccessMs())
    }

    @Test fun savesAreDebouncedIntoOneSync() {
        val rig = Rig()
        rig.unlock()
        rig.scheduler.advance(0)
        val epoch = rig.session.state.value.epoch
        repeat(3) {
            rig.session.saveItem("v1", "i1", ItemEdit("Edit $it", "", "", "", ""), epoch)
            rig.scheduler.advance(1_000)
        }
        assertEquals(1, rig.gateway.syncs.get())
        rig.scheduler.advance(SyncCoordinator.DEBOUNCE_MS)
        assertEquals(2, rig.gateway.syncs.get())
    }

    @Test fun periodicWhileUnlockedOnlyAndLockCancelsEverything() {
        val rig = Rig()
        rig.unlock()
        rig.scheduler.advance(0)
        rig.scheduler.advance(SyncCoordinator.PERIOD_MS)
        assertEquals(2, rig.gateway.syncs.get())
        rig.scheduler.advance(SyncCoordinator.PERIOD_MS)
        assertEquals(3, rig.gateway.syncs.get())
        rig.session.lock()
        assertEquals(0, rig.scheduler.pending())
        rig.scheduler.advance(SyncCoordinator.PERIOD_MS * 3)
        assertEquals(3, rig.gateway.syncs.get())
        rig.coordinator.syncNow()
        rig.scheduler.advance(0)
        assertEquals("no sync while locked", 3, rig.gateway.syncs.get())
    }

    @Test fun notConfiguredAndOfflineNeverCallTheEngine() {
        val rig = Rig(configured = false)
        assertEquals(SyncCoordinator.Phase.NotConfigured, rig.coordinator.status.value.phase)
        rig.unlock()
        rig.scheduler.advance(0)
        assertEquals(0, rig.gateway.syncs.get())
        assertEquals(SyncCoordinator.Phase.NotConfigured, rig.coordinator.status.value.phase)

        rig.settings.save(SyncTarget("FAKE-owner", "fake-vault", "main"), token)
        rig.online = false
        rig.coordinator.settingsChanged()
        rig.scheduler.advance(0)
        assertEquals(0, rig.gateway.syncs.get())
        assertEquals(SyncCoordinator.Phase.Offline, rig.coordinator.status.value.phase)
        rig.online = true
        rig.coordinator.syncNow()
        rig.scheduler.advance(0)
        assertEquals(1, rig.gateway.syncs.get())
    }

    @Test fun failuresAndConflictsAreReportedWithFixedText() {
        val rig = Rig()
        rig.gateway.failure = SyncFailedException("GitHub rejected the sync token (invalid or expired)")
        rig.unlock()
        rig.scheduler.advance(0)
        var s = rig.coordinator.status.value
        assertEquals(SyncCoordinator.Phase.Failed, s.phase)
        assertEquals("GitHub rejected the sync token (invalid or expired)", s.error)
        assertNull(s.lastSuccessMs)

        rig.gateway.failure = IllegalStateException("/data/user/0/app.gitpass.android/no_backup/vault $token")
        rig.coordinator.syncNow()
        rig.scheduler.advance(0)
        s = rig.coordinator.status.value
        assertEquals(SyncCoordinator.GENERIC_FAILURE, s.error)
        assertFalse(s.toString().contains("FAKE_TOKEN") || s.toString().contains("/data/"))

        rig.gateway.failure = null
        rig.gateway.outcome = SyncOutcome(true, true, listOf("v1/i1: remote deleted, local edited — kept local"), 1, 0)
        rig.coordinator.syncNow()
        rig.scheduler.advance(0)
        s = rig.coordinator.status.value
        assertEquals(SyncCoordinator.Phase.Idle, s.phase)
        assertEquals(1, s.conflicts.size)
        assertNull(s.error)
    }

    @Test fun localChangesFromSyncReloadTheList() {
        val rig = Rig()
        rig.gateway.onSync = { rig.gateway.titles["i2"] = "From desktop" }
        rig.gateway.outcome = SyncOutcome(false, true, emptyList(), 0, 0)
        rig.unlock()
        rig.scheduler.advance(0)
        assertTrue(rig.session.state.value.items.any { it.title == "From desktop" })
    }

    @Test fun nativeOpsFailFastDuringSyncAndLockCancelsWithoutWaiting() {
        val rig = Rig()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        var cancelledSeen = false
        rig.gateway.onSync = { cancel ->
            cancel.onCancel { cancelledSeen = true; release.countDown() }
            entered.countDown()
            release.await(5, TimeUnit.SECONDS)
        }
        rig.unlock()
        val worker = Thread { rig.scheduler.advance(0) }.apply { start() }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        assertTrue(rig.session.isSyncing())
        assertEquals(SyncCoordinator.Phase.Syncing, rig.coordinator.status.value.phase)
        assertThrows(SyncInProgressException::class.java) { rig.session.openDetail("i1") }
        assertThrows(SyncInProgressException::class.java) { rig.session.selectVault("v1") }
        // Lock does not wait for the network: it cancels the running sync.
        rig.session.lock()
        assertTrue(cancelledSeen)
        worker.join(5_000)
        assertFalse(worker.isAlive)
        assertFalse(rig.session.isSyncing())
        assertFalse(rig.session.state.value.unlocked)
        assertEquals(SyncCoordinator.Phase.Idle, rig.coordinator.status.value.phase)
        assertNull("a cancelled sync records no success", rig.settings.lastSuccessMs())
    }
}
