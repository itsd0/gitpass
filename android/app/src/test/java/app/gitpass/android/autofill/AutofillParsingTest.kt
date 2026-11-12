package app.gitpass.android.autofill

import app.gitpass.android.session.AutofillCredential
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FieldParserTest {
    private val textPassword = 0x1 or 0x80
    private val webPassword = 0x1 or 0xe0
    private val email = 0x1 or 0x20
    private val plain = 0x1

    @Test fun autofillHintsWin() {
        val parsed = FieldParser.parse(listOf(
            FieldNode("u", hints = listOf("emailAddress")),
            FieldNode("p", hints = listOf("password")),
        ))
        assertEquals(listOf("u"), parsed.usernames.map { it.id })
        assertEquals(listOf("p"), parsed.passwords.map { it.id })
    }

    @Test fun inputTypesAndHtml() {
        val parsed = FieldParser.parse(listOf(
            FieldNode("search", html = mapOf("type" to "search", "name" to "q")),
            FieldNode("mail", inputType = email),
            FieldNode("pw", inputType = textPassword),
            FieldNode("web", html = mapOf("type" to "password")),
            FieldNode("new", html = mapOf("autocomplete" to "new-password")),
        ))
        assertEquals(listOf("mail"), parsed.usernames.map { it.id })
        assertEquals(listOf("pw", "web", "new"), parsed.passwords.map { it.id })
    }

    @Test fun heuristicsAndFallbackUsername() {
        val byName = FieldParser.parse(listOf(FieldNode("a", inputType = plain, idEntry = "login_user"), FieldNode("b", inputType = webPassword)))
        assertEquals(listOf("a"), byName.usernames.map { it.id })
        val fallback = FieldParser.parse(listOf(FieldNode("title", inputType = plain, idEntry = "headline"), FieldNode("x", inputType = plain, idEntry = "field1"), FieldNode("p", idEntry = "pwd")))
        assertEquals("text field right before the password", listOf("x"), fallback.usernames.map { it.id })
        assertEquals(listOf("p"), fallback.passwords.map { it.id })
    }

    @Test fun otpSearchHiddenInvisibleAndNonTextAreIgnored() {
        val parsed = FieldParser.parse(listOf(
            FieldNode("otp", inputType = plain, idEntry = "otp_code"),
            FieldNode("one", html = mapOf("autocomplete" to "one-time-code")),
            FieldNode("hidden", html = mapOf("type" to "hidden", "name" to "password")),
            FieldNode("gone", hints = listOf("password"), isVisible = false),
            FieldNode("check", hints = listOf("password"), isText = false),
            FieldNode<String>(null, hints = listOf("password")),
        ))
        assertTrue(parsed.isEmpty)
    }

    @Test fun usernameOnlyStepIsOffered() {
        val parsed = FieldParser.parse(listOf(FieldNode("u", html = mapOf("autocomplete" to "username"))))
        assertEquals(listOf("u"), parsed.allIds)
    }

    @Test fun nothingOnPlainForms() {
        assertTrue(FieldParser.parse(listOf(FieldNode("n", inputType = plain, idEntry = "first_name"), FieldNode("c", inputType = plain, idEntry = "city"))).isEmpty)
    }
}

class AutofillTargetTest {
    @Test fun browsersUseTheWebOrigin() {
        assertEquals(AutofillTarget("https://accounts.example.com", null), AutofillTarget.of("com.android.chrome", "Accounts.Example.com.", null))
        assertEquals(AutofillTarget("https://example.com", null), AutofillTarget.of("com.mi.globalbrowser", "example.com", "https"))
        assertEquals(AutofillTarget("http://example.com", null), AutofillTarget.of("org.mozilla.firefox", "example.com", "http"))
        assertNull(AutofillTarget.of("org.mozilla.firefox", "example.com", "javascript"))
        assertNull(AutofillTarget.of("org.mozilla.firefox", "exa mple.com", null))
        assertNull("browser UI without a page", AutofillTarget.of("com.android.chrome", null, null))
    }

    @Test fun nonBrowsersNeverClaimAWebDomain() {
        assertEquals(AutofillTarget(null, "com.evil.app"), AutofillTarget.of("com.evil.app", "paypal.com", "https"))
        assertEquals(AutofillTarget(null, "com.example.app"), AutofillTarget.of("com.example.app", null, null))
    }

    @Test fun ownAppAndMissingPackageAreRefused() {
        assertNull(AutofillTarget.of(AutofillTarget.OWN_PACKAGE, null, null))
        assertNull(AutofillTarget.of(null, "example.com", null))
    }

    @Test fun labelsAndUrls() {
        assertEquals("example.com", AutofillTarget("https://example.com", null).label)
        assertEquals("https://example.com", AutofillTarget("https://example.com", null).itemUrl)
        assertEquals("android://com.example.app", AutofillTarget(null, "com.example.app").itemUrl)
    }
}

class FillPlanTest {
    @Test fun buildsValuesOnlyForPresentFields() {
        val logins = listOf(
            AutofillCredential("v", "1", "Both", "alice", "pw-FAKE"),
            AutofillCredential("v", "2", "No password", "bob", null),
            AutofillCredential("v", "3", "Empty", "", ""),
        )
        val plans = FillPlan.datasets(logins, listOf("u"), listOf("p"))
        assertEquals(listOf("Both", "No password"), plans.map { it.title })
        assertEquals(listOf("u" to "alice", "p" to "pw-FAKE"), plans[0].values)
        assertEquals(listOf("u" to "bob"), plans[1].values)
        assertFalse(plans[0].toString().contains("pw-FAKE"))
        assertTrue(FillPlan.datasets(logins, emptyList<String>(), emptyList()).isEmpty())
    }

    @Test fun capsTheDropdown() {
        val many = (1..50).map { AutofillCredential("v", "$it", "t$it", "u", "p") }
        assertEquals(FillPlan.MAX_DATASETS, FillPlan.datasets(many, listOf(1), listOf(2)).size)
    }
}

class SaveStashTest {
    @Test fun tokensAreSingleUseAndExpire() {
        var now = 0L
        val stash = SaveStash { now }
        val target = AutofillTarget("https://example.com", null)
        val token = stash.put(target, "alice", "pw-FAKE")
        assertEquals(32, token.length)
        assertFalse(token.contains("alice") || token.contains("pw-FAKE"))
        assertEquals("pw-FAKE", stash.take(token)!!.password)
        assertNull("single use", stash.take(token))
        val late = stash.put(target, "alice", "pw-FAKE")
        now += SaveStash.TTL_MS + 1
        assertNull("expired", stash.take(late))
        assertNull(stash.take(null))
        assertFalse(stash.put(target, "a", "b").let { stash.take(it).toString() }.contains("b\""))
    }
}
