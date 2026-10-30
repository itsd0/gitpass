import { useCallback, useEffect, useRef, useState } from 'react';
import { ExternalLink, EyeOff, KeyRound, ShieldCheck, SkipForward } from 'lucide-react';
import type { ItemInput, ItemSummary } from '../../api/types';
import { api } from '../../api/client';
import { SiteIcon } from '../../components/SiteIcon';
import { useT, type Key as TranslationKey } from '../../i18n';
import { Button, Modal, toast } from '../../ui';
import { useApp } from '../../lib/store';
import { useSettings } from '../settings/useSettings';
import { PasswordGenerator } from '../new-item/PasswordGenerator';
import { buildCheckupQueue, explanationKey, type CheckupFinding } from './checkup';

export const CHECKUP_RELOAD_EVENT = 'gitpass:watchtower-reload';

type Outcome = 'fixed' | 'ignored' | 'skipped';

/** Replace the first concealed (password) field with the generated value. */
async function applyGeneratedPassword(item: ItemSummary, pw: string): Promise<void> {
  const detail = await api.getItem({ vaultId: item.vaultId, itemId: item.id });
  let replaced = false;
  const sections = detail.sections.map((s) => ({
    ...s,
    fields: s.fields.map((f) => {
      if (!replaced && f.kind === 'concealed') {
        replaced = true;
        return { ...f, value: pw };
      }
      return f;
    }),
  }));
  const input: ItemInput = {
    vaultId: item.vaultId,
    category: detail.category,
    title: detail.title,
    sections,
    notes: detail.notes,
    tags: detail.tags,
    favorite: detail.favorite,
    url: detail.url ?? null,
    relatedItemIds: detail.relatedItemIds ?? [],
  };
  // Password history keeps the previous password (core behaviour on update).
  await api.updateItem({ itemId: item.id, input });
}

export function SecurityCheckup({ onClose }: { onClose: () => void }) {
  const t = useT();
  const settings = useSettings((st) => st.settings);
  const hibp = !!settings?.hibpCheck;
  const [queue, setQueue] = useState<CheckupFinding[] | null>(null);
  const [titles, setTitles] = useState<Record<string, ItemSummary>>({});
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [pos, setPos] = useState(0);
  const [results, setResults] = useState<Outcome[]>([]);
  const [scoreDelta, setScoreDelta] = useState<{ from: number; to: number } | null>(null);
  const [genOpen, setGenOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const lastScore = useRef<number | null>(null);
  const primaryRef = useRef<HTMLButtonElement>(null);

  // Load the report once and freeze the queue; later reloads only refresh the score.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [r, items] = await Promise.all([
          api.watchtower({ hibp }),
          api.listItems({ filter: { kind: 'all' } }),
        ]);
        if (cancelled) return;
        lastScore.current = r.score;
        setQueue(buildCheckupQueue(r));
        setTitles(Object.fromEntries(items.map((i) => [i.id, i])));
      } catch (e) {
        if (!cancelled) setLoadErr(String((e as Error)?.message ?? e));
      }
    })();
    return () => { cancelled = true; };
  }, [hibp]);

  const refreshScore = useCallback(async () => {
    try {
      const r = await api.watchtower({ hibp });
      if (lastScore.current !== null && r.score !== lastScore.current) {
        setScoreDelta({ from: lastScore.current, to: r.score });
      }
      lastScore.current = r.score;
    } catch {
      // score refresh is best-effort; the walk continues
    }
  }, [hibp]);

  const total = queue?.length ?? 0;
  const current: CheckupFinding | undefined = queue && pos < queue.length ? queue[pos] : undefined;
  const done = queue !== null && pos >= queue.length;
  const item = current ? titles[current.itemId] : undefined;
  const passwordFinding = current ? current.kind !== 'missingTotp' : false;

  // Focus the primary action whenever the finding advances.
  useEffect(() => {
    primaryRef.current?.focus();
  }, [pos, done]);

  const announce = done
    ? t('checkup.done')
    : current
    ? t('checkup.progress', { n: pos + 1, total })
    : '';

  const notifyChanged = () => {
    if (typeof window !== 'undefined') window.dispatchEvent(new Event(CHECKUP_RELOAD_EVENT));
  };

  const advance = useCallback((outcome: Outcome) => {
    setResults((r) => [...r, outcome]);
    setPos((p) => p + 1);
  }, []);

  const changeOnSite = async () => {
    if (!current || !item) return;
    setBusy(true);
    try {
      await api.openChangePassword({ vaultId: item.vaultId, itemId: item.id });
      advance('skipped'); // opened the site; nothing changed in the vault yet
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    } finally {
      setBusy(false);
    }
  };

  const ignore = async () => {
    if (!current || !item) return;
    setBusy(true);
    try {
      await api.ignoreFinding({ vaultId: item.vaultId, itemId: item.id, kind: current.ignoreKind });
      advance('ignored');
      notifyChanged();
      void refreshScore();
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    } finally {
      setBusy(false);
    }
  };

  const skip = () => advance('skipped');

  const onGenerated = async (pw: string) => {
    if (!current || !item) return;
    setGenOpen(false);
    setBusy(true);
    try {
      await applyGeneratedPassword(item, pw);
      toast(t('checkup.updatedToast'));
      advance('fixed');
      notifyChanged();
      void refreshScore();
    } catch (e) {
      toast(String((e as Error)?.message ?? e), 'danger');
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (genOpen || busy || done) return; // the generator popover owns its keys
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
    const k = e.key.toLowerCase();
    if (k === 's') {
      e.preventDefault();
      skip();
    } else if (k === 'i') {
      e.preventDefault();
      void ignore();
    } else if (e.key === 'Enter' && tag !== 'BUTTON' && tag !== 'A') {
      e.preventDefault();
      if (passwordFinding && item?.changePasswordUrl) void changeOnSite();
      else if (passwordFinding) setGenOpen(true);
    }
  };

  const counts = results.reduce(
    (acc, o) => ({ ...acc, [o]: acc[o] + 1 }),
    { fixed: 0, ignored: 0, skipped: 0 } as Record<Outcome, number>,
  );

  return (
    <Modal
      open
      onOpenChange={(o) => !o && onClose()}
      title={t('checkup.title')}
      className="w-[480px] max-w-full"
    >
      <div onKeyDown={onKeyDown} data-testid="checkup-body">
        <p role="status" aria-live="polite" className="sr-only">
          {announce}
        </p>
        {loadErr && (
          <p role="alert" className="text-sm text-danger">{loadErr}</p>
        )}
        {queue === null && !loadErr && <p className="text-sm text-muted">…</p>}
        {done && (
          <div className="space-y-4 text-center" data-testid="checkup-end">
            <ShieldCheck size={32} className="mx-auto text-ok" />
            <p className="font-semibold">{t('checkup.done')}</p>
            <p className="text-sm text-muted" data-testid="checkup-summary">
              {t('checkup.summary', { fixed: counts.fixed, ignored: counts.ignored, skipped: counts.skipped })}
            </p>
            {scoreDelta && (
              <p className="text-sm font-medium text-accent" data-testid="checkup-score-delta">
                {t('checkup.scoreDelta', { from: scoreDelta.from, to: scoreDelta.to })}
              </p>
            )}
            <Button ref={primaryRef} onClick={onClose}>{t('checkup.close')}</Button>
          </div>
        )}
        {current && (
          <div className="space-y-4" data-testid="checkup-step">
            <div className="flex items-center justify-between text-xs text-muted">
              <span data-testid="checkup-progress">{t('checkup.progress', { n: pos + 1, total })}</span>
              {scoreDelta && (
                <span className="font-medium text-accent" data-testid="checkup-score-delta">
                  {t('checkup.scoreDelta', { from: scoreDelta.from, to: scoreDelta.to })}
                </span>
              )}
            </div>
            <div className="flex items-center gap-3">
              <SiteIcon item={item} title={item?.title} size={28} />
              <div className="min-w-0">
                <p className="truncate font-semibold" data-testid="checkup-item-title">{item?.title ?? current.itemId}</p>
                <p className="truncate text-sm text-muted">{item?.subtitle}</p>
              </div>
            </div>
            <p className="text-sm" data-testid="checkup-explanation">
              <span className="font-medium">{t(`watchtower.${{ breached: 'breachedPasswords', reused: 'reusedPasswords', weak: 'weakPasswords', old: 'oldPasswords', missingTotp: 'missingTotp' }[current.kind]}` as TranslationKey)}</span>
              {' — '}
              {t(explanationKey(current.kind) as TranslationKey)}
            </p>
            <div className="flex flex-wrap gap-2">
              {passwordFinding && item?.changePasswordUrl && (
                <Button ref={primaryRef} size="sm" disabled={busy} onClick={changeOnSite} data-testid="checkup-change-on-site">
                  <ExternalLink size={13} /> {t('checkup.changeOnSite')}
                </Button>
              )}
              {passwordFinding && (
                <Button
                  ref={item?.changePasswordUrl ? undefined : primaryRef}
                  variant={item?.changePasswordUrl ? 'outline' : 'default'}
                  size="sm"
                  disabled={busy}
                  onClick={() => setGenOpen(true)}
                  data-testid="checkup-generate-update"
                >
                  <KeyRound size={13} /> {t('checkup.generateUpdate')}
                </Button>
              )}
              {!passwordFinding && (
                <Button
                  ref={primaryRef}
                  size="sm"
                  disabled={busy}
                  onClick={() => {
                    const { selectItem, setRoute, setEditing } = useApp.getState();
                    setRoute({ name: 'items', filter: item ? { kind: 'vault', vaultId: item.vaultId } : { kind: 'all' } });
                    selectItem(current.itemId);
                    setEditing(true);
                    onClose();
                  }}
                  data-testid="checkup-open-item"
                >
                  {t('checkup.openItem')}
                </Button>
              )}
              <Button variant="ghost" size="sm" disabled={busy} onClick={ignore} data-testid="checkup-ignore">
                <EyeOff size={13} /> {t('checkup.ignore')}
              </Button>
              <Button variant="ghost" size="sm" disabled={busy} onClick={skip} data-testid="checkup-skip">
                <SkipForward size={13} /> {t('checkup.skip')}
              </Button>
            </div>
            <p className="text-xs text-muted">{t('checkup.keysHint')}</p>
          </div>
        )}
      </div>
      <PasswordGenerator open={genOpen} onOpenChange={setGenOpen} onUse={(pw) => void onGenerated(pw)} />
    </Modal>
  );
}

export function SecurityCheckupModalHost() {
  const { modal, closeModal } = useApp();
  if (modal?.id !== 'security-checkup') return null;
  return <SecurityCheckup onClose={closeModal} />;
}
