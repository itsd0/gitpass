import { useState } from 'react';
import { ChevronRight, Eye, EyeOff } from 'lucide-react';
import { api } from '../../../api/client';
import type { ItemDetail as Detail } from '../../../api/types';
import { useApp } from '../../../lib/store';
import { formatRelativeLastUsed } from '../lastUsedAge';
import { groupCardNumber } from '../../../lib/cardMask';
import { cn, fmtDateLong } from '../../../lib/utils';
import { Button, Dialog, DialogContent, toast } from '../../../ui';
import { useT } from '../../../i18n';
import { errText } from '../actions';

export function DetailHistory({ detail }: { detail: Detail }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<Detail | null>(null);
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const versions = [...detail.history].sort((a, b) => b.version - a.version);

  const openSnapshot = (ver: number) => {
    setConfirming(false);
    setRevealed({});
    setSelectedVersion(ver);
    api.itemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: ver })
      .then(setView, (e) => toast(errText(e), 'danger'));
  };

  const toggleReveal = async (fieldId: string) => {
    if (revealed[fieldId] !== undefined) {
      setRevealed((prev) => {
        const next = { ...prev };
        delete next[fieldId];
        return next;
      });
      return;
    }
    try {
      const val = await api.revealField({
        vaultId: detail.vaultId,
        itemId: detail.id,
        fieldId,
        version: selectedVersion,
      });
      setRevealed((prev) => ({ ...prev, [fieldId]: val }));
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const handleRestore = async () => {
    if (!view || selectedVersion === null) return;
    const currentVersionBefore = detail.version ?? (detail.history[0]?.version ? detail.history[0].version + 1 : 1);
    const targetVersion = selectedVersion;
    setView(null);
    setConfirming(false);
    try {
      await api.restoreItemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: targetVersion });
      useApp.getState().refreshItems();
      toast(t('detail.toast.versionRestored', { version: targetVersion }), {
        duration: 6000,
        action: {
          label: t('common.undo'),
          altText: t('common.undo'),
          onClick: async () => {
            try {
              await api.restoreItemVersion({ vaultId: detail.vaultId, itemId: detail.id, version: currentVersionBefore });
              useApp.getState().refreshItems();
              toast(t('detail.toast.versionUndone'));
            } catch (e) {
              toast(errText(e), 'danger');
            }
          },
        },
      });
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  return (
    <div className="mt-2 text-sm" data-testid="history">
      <button className="flex items-center gap-1.5 text-fg/90 hover:text-fg" onClick={() => setOpen(!open)} aria-expanded={open}>
        <ChevronRight size={14} className={cn('transition-transform', open && 'rotate-90')} />{t('detail.lastEdited', { date: fmtDateLong(detail.updatedAt) })}
      </button>
      {open && (
        <div className="ml-5 mt-2 space-y-1">
          <div className="text-xs text-muted">{t('detail.created', { date: fmtDateLong(detail.createdAt) })}</div>
          {detail.lastUsed ? (
            <div className="text-xs text-muted">
              {formatRelativeLastUsed(detail.lastUsed, undefined, t)}
            </div>
          ) : null}
          {versions.map((h) => (
            <button key={h.version} className="block text-xs text-accent hover:underline"
              onClick={() => openSnapshot(h.version)}>
              {t('detail.versionHistory', { version: h.version, date: fmtDateLong(h.updatedAt) })}
            </button>
          ))}
        </div>
      )}
      <Dialog open={!!view} onOpenChange={(o) => { if (!o) { setView(null); setConfirming(false); } }}>
        <DialogContent
          title={view && selectedVersion !== null ? t('detail.versionSnapshotTitle', { version: selectedVersion, title: view.title }) : ''}
          description={t('detail.versionSnapshotDesc')}
        >
          {view && (
            <div className="space-y-4">
              <div className="max-h-80 overflow-y-auto rounded-lg border border-border">
                {view.sections.flatMap((s) => s.fields).map((f) => {
                  const isConcealed = f.kind === 'concealed' || f.kind === 'totp';
                  const isRevealed = revealed[f.id] !== undefined;
                  const isApi = view.category === 'api_credential';
                  const shouldGroup = !isApi && Boolean(f.masked);
                  const displayValue = isConcealed
                    ? (isRevealed ? (shouldGroup ? groupCardNumber(revealed[f.id]) : revealed[f.id]) : (f.masked || '••••••••••'))
                    : (f.value ?? '');
                  return (
                    <div key={f.id} className="flex items-center justify-between border-b border-border px-3 py-2 last:border-0">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="text-xs text-label">{f.label}</span>
                          {f.diff === 'changed' && (
                            <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-600 dark:text-amber-400 border border-amber-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.changed')}
                            </span>
                          )}
                          {f.diff === 'added' && (
                            <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400 border border-emerald-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.added')}
                            </span>
                          )}
                          {f.diff === 'removed' && (
                            <span className="rounded bg-rose-500/10 px-1.5 py-0.5 text-[10px] font-medium text-rose-600 dark:text-rose-400 border border-rose-500/20" data-testid={`diff-${f.id}`}>
                              {t('detail.diff.removed')}
                            </span>
                          )}
                        </div>
                        <div className={cn("font-mono text-sm", isConcealed && !isRevealed && "tracking-widest")}>
                          {displayValue}
                        </div>
                      </div>
                      {isConcealed && (
                        <Button
                          variant="ghost"
                          size="iconSm"
                          aria-label={isRevealed ? `Conceal ${f.label}` : `Reveal ${f.label}`}
                          onClick={() => toggleReveal(f.id)}
                        >
                          {isRevealed ? <EyeOff size={13} /> : <Eye size={13} />}
                        </Button>
                      )}
                    </div>
                  );
                })}
              </div>

              {confirming ? (
                <div className="flex flex-col gap-2 rounded-lg border border-border bg-hover/50 p-3" data-testid="restore-confirm-pane">
                  <div className="text-sm font-medium text-fg">
                    {t('detail.restoreVersionConfirmTitle', { version: selectedVersion ?? 0 })}
                  </div>
                  <div className="text-xs text-muted">
                    {t('detail.restoreVersionConfirmDesc', { version: selectedVersion ?? 0 })}
                  </div>
                  <div className="mt-2 flex justify-end gap-2">
                    <Button variant="outline" size="sm" onClick={() => setConfirming(false)}>
                      {t('common.cancel')}
                    </Button>
                    <Button variant="default" size="sm" onClick={handleRestore} data-testid="confirm-restore-btn">
                      {t('detail.restoreVersionConfirmBtn')}
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="flex justify-end gap-2">
                  <Button variant="outline" onClick={() => setView(null)}>
                    {t('common.close')}
                  </Button>
                  <Button variant="default" onClick={() => setConfirming(true)} data-testid="restore-version-btn">
                    {t('detail.restoreVersion')}
                  </Button>
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

export { DetailHistory as History };
