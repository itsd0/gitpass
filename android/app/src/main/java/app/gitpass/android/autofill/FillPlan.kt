package app.gitpass.android.autofill

import app.gitpass.android.session.AutofillCredential

/** One dataset to offer: a label and the field values it fills. */
class DatasetPlan<T>(val title: String, val subtitle: String, val values: List<Pair<T, String>>) {
    override fun toString() = "DatasetPlan([redacted])"
}

/**
 * Turns matching logins into datasets (#475). Pure, so it is unit-tested
 * without the framework. Only called after an unlock, only with logins that
 * gitpass-core already matched to the request.
 */
object FillPlan {
    fun <T> datasets(logins: List<AutofillCredential>, usernameIds: List<T>, passwordIds: List<T>): List<DatasetPlan<T>> =
        logins.mapNotNull { login ->
            val values = mutableListOf<Pair<T, String>>()
            login.username?.takeIf { it.isNotEmpty() }?.let { u -> usernameIds.forEach { values += it to u } }
            login.password?.takeIf { it.isNotEmpty() }?.let { p -> passwordIds.forEach { values += it to p } }
            if (values.isEmpty()) null else DatasetPlan(login.title, login.username.orEmpty(), values)
        }.take(MAX_DATASETS)

    /** The framework shows a limited dropdown; keep the response small. */
    const val MAX_DATASETS = 20
}
