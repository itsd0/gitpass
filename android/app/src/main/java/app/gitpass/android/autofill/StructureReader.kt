package app.gitpass.android.autofill

import android.app.assist.AssistStructure
import android.view.View
import android.view.autofill.AutofillId

/** What one request's view structure contains, flattened for [FieldParser] and [AutofillTarget]. */
class StructureSnapshot(val packageName: String?, val webDomain: String?, val webScheme: String?, val nodes: List<FieldNode<AutofillId>>)

/** Flattens an [AssistStructure] (#475). Reads typed text only when [withText] (save requests). */
object StructureReader {
    fun read(structure: AssistStructure, withText: Boolean = false): StructureSnapshot {
        val nodes = mutableListOf<FieldNode<AutofillId>>()
        var webDomain: String? = null
        var webScheme: String? = null
        for (i in 0 until structure.windowNodeCount) {
            val root = structure.getWindowNodeAt(i).rootViewNode ?: continue
            walk(root, nodes, withText) { domain, scheme ->
                if (webDomain == null && !domain.isNullOrBlank()) {
                    webDomain = domain
                    webScheme = scheme
                }
            }
        }
        return StructureSnapshot(structure.activityComponent?.packageName, webDomain, webScheme, nodes)
    }

    private fun walk(
        node: AssistStructure.ViewNode,
        out: MutableList<FieldNode<AutofillId>>,
        withText: Boolean,
        onWeb: (String?, String?) -> Unit,
    ) {
        onWeb(node.webDomain, if (android.os.Build.VERSION.SDK_INT >= 28) node.webScheme else null)
        if (node.autofillType != View.AUTOFILL_TYPE_NONE || node.htmlInfo != null) {
            val html = node.htmlInfo?.takeIf { it.tag.equals("input", ignoreCase = true) }?.attributes
                ?.associate { it.first.lowercase() to it.second.orEmpty() }
                .orEmpty()
            out += FieldNode(
                id = node.autofillId,
                hints = node.autofillHints?.toList().orEmpty(),
                inputType = node.inputType,
                idEntry = node.idEntry,
                hint = node.hint,
                html = html,
                isText = node.autofillType == View.AUTOFILL_TYPE_TEXT,
                isVisible = node.visibility == View.VISIBLE,
                text = if (withText) node.autofillValue?.takeIf { it.isText }?.textValue?.toString() else null,
            )
        }
        for (i in 0 until node.childCount) walk(node.getChildAt(i), out, withText, onWeb)
    }
}
