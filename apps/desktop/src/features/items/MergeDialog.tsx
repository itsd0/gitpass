import { useEffect, useMemo, useState } from "react";
import { api } from "../../api/client";
import type { ItemDetail, ItemSummary, MergeDuplicatePreview } from "../../api/types";
import { useT } from "../../i18n";
import { useApp } from "../../lib/store";
import { Button, Modal, toast } from "../../ui";
import { formatRelativeLastUsed } from "./lastUsedAge";
import { errText } from "./actions";

/** Recency score for the default kept item: last use wins, else last edit. */
export function mergeRecency(it: ItemSummary): number {
  if (it.lastUsed) return it.lastUsed > 1e11 ? it.lastUsed : it.lastUsed * 1000;
  return Date.parse(it.updatedAt) || 0;
}

function SideColumn({ detail, checked, onKeep, keepLabel }: { detail: ItemDetail; checked: boolean; onKeep: () => void; keepLabel: string }) {
  const t = useT();
  return (
    <div data-testid={`merge-column-${detail.id}`} className={`rounded-lg border p-3 ${checked ? "border-accent" : "border-border"}`}>
      <label className="flex cursor-pointer items-center gap-2 text-sm font-medium">
        <input type="radio" name="merge-keep" checked={checked} onChange={onKeep} className="accent-[var(--accent)]" />
        {keepLabel}
      </label>
      <div className="mt-2 truncate font-semibold">{detail.title}</div>
      {detail.subtitle && <div className="truncate text-xs text-muted">{detail.subtitle}</div>}
      {detail.url && <div className="truncate text-xs text-muted">{detail.url}</div>}
      <div className="mt-2 text-xs text-muted">
        {detail.tags.length ? detail.tags.join(", ") : t("merge.noTags")}
      </div>
      <div className="mt-1 text-xs text-muted">
        {detail.lastUsed ? formatRelativeLastUsed(detail.lastUsed, new Date(), t) : t("merge.neverUsed")}
      </div>
      <div className="text-xs text-muted">
        {t("merge.updatedOn", { date: detail.updatedAt ? new Date(detail.updatedAt).toLocaleDateString() : "" })}
      </div>
      {detail.hasTotp && <div className="mt-1 text-xs text-muted">{t("merge.hasTotp")}</div>}
      {detail.attachments.length > 0 && (
        <div className="text-xs text-muted">{t("merge.attachments", { n: detail.attachments.length })}</div>
      )}
    </div>
  );
}

/**
 * Merge two duplicate logins (#396): side-by-side comparison, a keep radio
 * (default: most recently used), a secrets-free preview from the backend,
 * Confirm merges, and the toast offers Undo (restore the discarded item,
 * then restore the kept item to its pre-merge version).
 */
export function MergeDialog({ open, onOpenChange, vaultId, itemA, itemB, onMerged }: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  vaultId: string;
  itemA: ItemSummary;
  itemB: ItemSummary;
  onMerged?: () => void;
}) {
  const t = useT();
  const [keepId, setKeepId] = useState<string>(() =>
    mergeRecency(itemB) > mergeRecency(itemA) ? itemB.id : itemA.id);
  const [details, setDetails] = useState<[ItemDetail, ItemDetail] | null>(null);
  const [preview, setPreview] = useState<MergeDuplicatePreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setKeepId(mergeRecency(itemB) > mergeRecency(itemA) ? itemB.id : itemA.id);
    setDetails(null);
    setPreview(null);
    setErr(null);
    let cancelled = false;
    Promise.all([
      api.getItem({ vaultId, itemId: itemA.id }),
      api.getItem({ vaultId, itemId: itemB.id }),
    ])
      .then(([a, b]) => { if (!cancelled) setDetails([a, b]); })
      .catch((e) => { if (!cancelled) setErr(errText(e)); });
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, vaultId, itemA.id, itemB.id]);

  useEffect(() => {
    if (!open) return;
    const otherId = keepId === itemA.id ? itemB.id : itemA.id;
    setPreview(null);
    api.previewMergeDuplicates({ vaultId, keepId, otherId })
      .then(setPreview)
      .catch((e) => setErr(errText(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, keepId, vaultId, itemA.id, itemB.id]);

  const keepDetail = useMemo(
    () => (details ? (keepId === itemA.id ? details[0] : details[1]) : null),
    [details, keepId, itemA.id],
  );
  const otherDetail = useMemo(
    () => (details ? (keepId === itemA.id ? details[1] : details[0]) : null),
    [details, keepId, itemA.id],
  );

  const previewLines = useMemo(() => {
    if (!preview) return [];
    const r = preview.report;
    const lines: string[] = [t("merge.previewTitle", { title: preview.mergedTitle })];
    if (preview.mergedUrl) lines.push(t("merge.previewUrl", { url: preview.mergedUrl }));
    if (preview.mergedTags.length) lines.push(t("merge.previewTags", { tags: preview.mergedTags.join(", ") }));
    if (r.passwordFromOther) lines.push(t("merge.previewPasswordFromOther"));
    else if (r.passwordMovedToHistory) lines.push(t("merge.previewPasswordKeptHistory"));
    else lines.push(t("merge.previewPasswordKept"));
    if (r.totpFromOther) lines.push(t("merge.previewTotpFromOther"));
    if (r.urlsAdded) lines.push(t("merge.previewUrlsAdded", { n: r.urlsAdded }));
    if (r.tagsAdded.length) lines.push(t("merge.previewTagsAdded", { tags: r.tagsAdded.join(", ") }));
    if (r.relatedAdded) lines.push(t("merge.previewRelatedAdded", { n: r.relatedAdded }));
    if (r.notesJoined) lines.push(t("merge.previewNotesJoined"));
    if (r.fieldsCopied.length) lines.push(t("merge.previewFieldsCopied", { fields: r.fieldsCopied.join(", ") }));
    if (r.fieldsConflicted.length) lines.push(t("merge.previewFieldsConflicted", { fields: r.fieldsConflicted.join(", ") }));
    if (r.attachmentsAdded) lines.push(t("merge.previewAttachmentsAdded", { n: r.attachmentsAdded }));
    return lines;
  }, [preview, t]);

  const confirm = async () => {
    if (!keepDetail || !otherDetail) return;
    const otherId = otherDetail.id;
    setBusy(true);
    setErr(null);
    try {
      const result = await api.mergeDuplicates({ vaultId, keepId, otherId });
      const keptTitle = result.kept.title;
      toast(t("merge.mergedToast", { title: keptTitle }), {
        duration: 5000,
        action: {
          label: t("common.undo"),
          altText: t("common.undo"),
          onClick: async () => {
            try {
              await api.restoreItem({ vaultId, itemId: otherId });
              await api.restoreItemVersion({ vaultId, itemId: keepId, version: result.keepVersionBefore });
              toast(t("merge.undoneToast"));
              useApp.getState().refreshItems();
              onMerged?.();
            } catch (e) {
              toast(errText(e), "danger");
            }
          },
        },
      });
      useApp.getState().refreshItems();
      onMerged?.();
      onOpenChange(false);
    } catch (e) {
      setErr(errText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal open={open} onOpenChange={onOpenChange} title={t("merge.title")} description={t("merge.description")} wide>
      <form
        onSubmit={(e) => { e.preventDefault(); void confirm(); }}
        aria-label={t("merge.title")}
      >
        {err && <p role="alert" className="mb-3 text-sm text-danger">{err}</p>}
        {details && keepDetail && otherDetail ? (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              <SideColumn
                detail={details[0]}
                checked={keepId === itemA.id}
                onKeep={() => setKeepId(itemA.id)}
                keepLabel={t("merge.keepThis")}
              />
              <SideColumn
                detail={details[1]}
                checked={keepId === itemB.id}
                onKeep={() => setKeepId(itemB.id)}
                keepLabel={t("merge.keepThis")}
              />
            </div>
            <div className="mt-4 rounded-lg bg-bg p-3">
              <div className="mb-1 text-xs font-medium text-muted">{t("merge.previewHeading")}</div>
              {preview ? (
                <ul className="space-y-1 text-xs">
                  {previewLines.map((line, i) => <li key={i}>{line}</li>)}
                </ul>
              ) : (
                <p className="text-xs text-muted">{t("merge.previewLoading")}</p>
              )}
            </div>
            <p className="mt-3 text-xs text-muted">{t("merge.discardedNote", { title: otherDetail.title })}</p>
          </>
        ) : (
          <p className="text-sm text-muted">{t("merge.loading")}</p>
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t("common.cancel")}</Button>
          <Button type="submit" disabled={busy || !details || !preview}>{t("merge.confirm")}</Button>
        </div>
      </form>
    </Modal>
  );
}
