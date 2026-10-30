import React, { useEffect, useReducer, useRef, useState } from 'react';
import { ArrowLeft, CheckCircle2, FileText, FileUp, FolderOpen } from 'lucide-react';
import clsx from 'clsx';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { Button, Input } from '../../ui';
import { useT, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import {
  IMPORT_FORMATS,
  canPreview,
  detectImportFormat,
  importReducer,
  initialImport,
  isPasswordProtectedBitwarden,
  type ImportFormatOption,
  type ImportStep,
} from './importWizardLogic';

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function ImportWizard({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const t = useT();
  const [s, dispatch] = useReducer(importReducer, initialImport());
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [busy, setBusy] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    headingRef.current?.focus();
  }, [s.step]);

  useEffect(() => {
    api
      .listVaults()
      .then((v) => {
        setVaults(v);
        if (v[0]) dispatch({ type: 'vault', vaultId: v[0].id });
      })
      .catch((e) => dispatch({ type: 'error', error: String(e) }));
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) => {
        if (cancelled) return;
        return getCurrentWebview().onDragDropEvent(async (event) => {
          if (event.payload.type === 'over' || event.payload.type === 'enter') {
            setIsDragging(true);
          } else if (event.payload.type === 'drop') {
            setIsDragging(false);
            const path = event.payload.paths?.[0];
            if (path) {
              await applyFile(path);
            }
          } else {
            setIsDragging(false);
          }
        });
      })
      .then((fn) => {
        if (cancelled) {
          fn?.();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const run = async (dryRun: boolean) => {
    if (!canPreview(s)) return;
    setBusy(true);
    try {
      const r = await api.importFile({
        vaultId: s.vaultId!,
        format: s.format!,
        path: s.path.trim(),
        dryRun,
        password: s.format === 'bitwarden-json' ? s.password : undefined,
      });
      dispatch(dryRun ? { type: 'preview', preview: r } : { type: 'done', result: r });
      if (!dryRun) {
        api.setOnboarding({ patch: { imported: true } }).catch(() => {});
      }
    } catch (e) {
      const error = String(e);
      dispatch({ type: 'error', error });
      // A file picked by path gives us no content snippet: reveal the password step on demand.
      if (s.format === 'bitwarden-json' && error.includes('export password is required')) {
        dispatch({ type: 'needsPassword' });
      }
    } finally {
      setBusy(false);
    }
  };

  const applyFile = async (path: string, fileObj?: File) => {
    let snippet = '';
    if (fileObj) {
      try {
        snippet = await fileObj.slice(0, 4096).text();
      } catch {}
    }
    const detected = detectImportFormat(path, snippet);
    dispatch({
      type: 'file',
      path,
      format: detected ?? s.format,
      autoDetected: !!detected,
      needsPassword: detected === 'bitwarden-json' && isPasswordProtectedBitwarden(snippet),
    });
  };

  const handleChooseFile = async () => {
    if (isTauri()) {
      try {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const selected = await open({
          multiple: false,
          filters: [
            { name: 'Export files', extensions: ['csv', 'json', '1pux'] },
          ],
        });
        if (typeof selected === 'string') {
          await applyFile(selected);
        }
      } catch (e) {
        dispatch({ type: 'error', error: String(e) });
      }
    } else {
      fileInputRef.current?.click();
    }
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    const file = e.dataTransfer.files?.[0];
    if (!file) return;
    const path = (file as any).path || file.name;
    await applyFile(path, file);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onDone();
    } else if (e.key === 'Enter') {
      const target = e.target as HTMLElement | null;
      if (target?.closest?.('button, a, select, textarea')) return;
      if (s.step === 'file') {
        if (canPreview(s) && !busy) {
          e.preventDefault();
          run(true);
        }
      } else if (s.step === 'preview') {
        if (!busy) {
          e.preventDefault();
          run(false);
        }
      } else if (s.step === 'done') {
        e.preventDefault();
        onDone();
      }
    }
  };

  const handleViewImported = () => {
    const { setRoute, refreshItems } = useApp.getState();
    if (s.vaultId) {
      setRoute({ name: 'items', filter: { kind: 'vault', vaultId: s.vaultId } });
    } else {
      setRoute({ name: 'items', filter: { kind: 'all', sort: 'updated' } });
    }
    refreshItems?.();
    onDone();
  };

  const selectedFormat = IMPORT_FORMATS.find((f) => f.id === s.format) as
    | ImportFormatOption
    | undefined;

  const steps: { id: ImportStep; labelKey: Key }[] = [
    { id: 'file', labelKey: 'import.step1' },
    { id: 'preview', labelKey: 'import.step2' },
    { id: 'done', labelKey: 'import.step3' },
  ];

  return (
    <div aria-label={t('import.title')} onKeyDown={handleKeyDown}>
      <div className="mb-4 flex items-center justify-between border-b border-border pb-3">
        <div className="flex items-center gap-2">
          {s.step !== 'done' && (
            <Button
              size="icon"
              variant="ghost"
              aria-label={t('import.back')}
              onClick={() => (s.step === 'file' ? onBack() : dispatch({ type: 'back' }))}
            >
              <ArrowLeft size={16} />
            </Button>
          )}
          <h3
            ref={headingRef}
            tabIndex={-1}
            className="text-lg font-semibold outline-none"
          >
            {t('import.title')}
          </h3>
        </div>
        <div className="flex items-center gap-1.5 text-xs" aria-label={t('import.stepsLabel')}>
          {steps.map((st, i) => {
            const active = s.step === st.id;
            const done =
              (st.id === 'file' && s.step !== 'file') ||
              (st.id === 'preview' && s.step === 'done');
            return (
              <div key={st.id} className="flex items-center gap-1.5">
                {i > 0 && <span className="text-muted/40">/</span>}
                <span
                  className={clsx(
                    active
                      ? 'font-medium text-accent'
                      : done
                      ? 'text-fg'
                      : 'text-muted'
                  )}
                >
                  {i + 1}. {t(st.labelKey)}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      <input
        ref={fileInputRef}
        type="file"
        data-testid="file-input"
        accept=".csv,.json,.1pux"
        className="hidden"
        aria-label={t('import.chooseFile')}
        onChange={async (e) => {
          const file = e.target.files?.[0];
          if (!file) return;
          const path = (file as any).path || file.name;
          await applyFile(path, file);
        }}
      />

      {s.step === 'file' && (
        <div className="space-y-4">
          <div
            data-testid="drop-zone"
            onDragOver={(e) => {
              e.preventDefault();
              setIsDragging(true);
            }}
            onDragEnter={(e) => {
              e.preventDefault();
              setIsDragging(true);
            }}
            onDragLeave={(e) => {
              e.preventDefault();
              setIsDragging(false);
            }}
            onDrop={handleDrop}
            className={clsx(
              'flex flex-col items-center justify-center rounded-xl border-2 border-dashed p-6 text-center transition-colors',
              isDragging
                ? 'border-accent bg-accent/10'
                : 'border-border bg-hover/40 hover:border-accent/50'
            )}
          >
            <FileUp
              size={28}
              className={isDragging ? 'text-accent' : 'mb-2 text-muted'}
            />
            <p className="text-sm font-medium text-fg">{t('import.dropzoneTitle')}</p>
            <p className="my-1 text-xs text-muted">{t('import.dropzoneOr')}</p>
            <Button
              type="button"
              variant="default"
              size="sm"
              onClick={handleChooseFile}
            >
              <FolderOpen size={14} /> {t('import.chooseFile')}
            </Button>
            <p className="mt-2 text-[11px] text-muted">{t('import.dropzoneHint')}</p>
          </div>

          {s.path && (
            <div className="flex items-center justify-between rounded-lg border border-border bg-panel p-2.5 text-xs">
              <div className="flex items-center gap-2 overflow-hidden">
                <FileText size={16} className="shrink-0 text-accent" />
                <span className="truncate font-mono text-fg">{s.path}</span>
              </div>
              {s.autoDetected && s.format && (
                <span className="shrink-0 rounded-full bg-accent/15 px-2 py-0.5 text-[10px] font-medium text-accent">
                  {t('import.formatAutoDetected')}
                </span>
              )}
            </div>
          )}

          <div>
            <label className="block text-xs text-muted">
              {t('import.manualPath')}
              <Input
                className="mt-1 font-mono text-xs"
                aria-label={t('import.manualPath')}
                placeholder={t('import.manualPathPlaceholder')}
                value={s.path}
                onChange={(e) => dispatch({ type: 'path', path: e.target.value })}
              />
            </label>
          </div>

          {s.needsPassword && s.format === 'bitwarden-json' && (
            <div className="rounded-lg border border-accent/30 bg-accent/10 p-2.5" data-testid="export-password">
              <label className="block text-xs font-medium text-fg">
                {t('import.exportPassword')}
                <Input
                  type="password"
                  autoComplete="off"
                  autoFocus
                  className="mt-1"
                  aria-label={t('import.exportPassword')}
                  value={s.password}
                  onChange={(e) => dispatch({ type: 'password', password: e.target.value })}
                />
              </label>
              <p className="mt-1 text-[11px] text-muted">{t('import.exportPasswordHint')}</p>
            </div>
          )}

          <div className="grid grid-cols-2 gap-3">
            <label className="block text-xs text-muted">
              {t('import.format')}
              <select
                aria-label={t('import.format')}
                className="mt-1 h-9 w-full rounded-md border border-border bg-panel px-2 text-sm text-fg"
                value={s.format ?? ''}
                onChange={(e) => dispatch({ type: 'format', format: e.target.value })}
              >
                <option value="" disabled>
                  {t('import.selectFormat')}
                </option>
                {IMPORT_FORMATS.map((f) => (
                  <option key={f.id} value={f.id}>
                    {t(f.labelKey)}
                  </option>
                ))}
              </select>
            </label>

            <label className="block text-xs text-muted">
              {t('import.vault')}
              <select
                aria-label={t('import.vault')}
                className="mt-1 h-9 w-full rounded-md border border-border bg-panel px-2 text-sm text-fg"
                value={s.vaultId ?? ''}
                onChange={(e) => dispatch({ type: 'vault', vaultId: e.target.value })}
              >
                {vaults.map((v) => (
                  <option key={v.id} value={v.id}>
                    {v.name}
                  </option>
                ))}
              </select>
            </label>
          </div>

          {selectedFormat?.hintKey && (
            <p
              data-testid="format-hint"
              className="rounded-md border border-accent/30 bg-accent/10 px-2.5 py-1.5 text-xs text-fg"
            >
              {t(selectedFormat.hintKey)}
            </p>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <Button variant="ghost" onClick={onBack}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="default"
              disabled={!canPreview(s) || busy}
              onClick={() => run(true)}
            >
              <FileUp size={14} /> {busy ? t('import.checking') : t('import.previewButton')}
            </Button>
          </div>
        </div>
      )}

      {s.step === 'preview' && s.preview && (
        <div className="space-y-4">
          <div className="space-y-4 rounded-xl border border-border bg-panel p-4">
            <div>
              <p className="text-sm font-medium text-fg">{t('import.previewTitle')}</p>
              <p className="text-xs text-muted">{t('import.previewDescription')}</p>
            </div>

            <dl className="grid grid-cols-3 gap-2 text-center">
              <div className="rounded-lg bg-hover p-3">
                <dt className="text-xs text-muted">{t('import.parsed')}</dt>
                <dd className="text-2xl font-semibold text-fg" data-testid="preview-Parsed">
                  {s.preview.parsed}
                </dd>
              </div>
              <div className="rounded-lg bg-hover p-3">
                <dt className="text-xs text-muted">{t('import.willAdd')}</dt>
                <dd className="text-2xl font-semibold text-ok" data-testid="preview-Will add">
                  {s.preview.parsed - s.preview.duplicates}
                </dd>
              </div>
              <div className="rounded-lg bg-hover p-3">
                <dt className="text-xs text-muted">{t('import.duplicates')}</dt>
                <dd className="text-2xl font-semibold text-muted" data-testid="preview-Duplicates skipped">
                  {s.preview.duplicates}
                </dd>
              </div>
            </dl>

            {s.preview.categories && Object.keys(s.preview.categories).length > 0 && (
              <div>
                <span className="text-xs font-medium text-muted">{t('import.itemTypes')}</span>
                <div className="mt-1.5 flex flex-wrap gap-1.5 text-xs" aria-label={t('import.itemTypes')}>
                  {Object.entries(s.preview.categories).map(([c, n]) => (
                    <span
                      key={c}
                      className="rounded-full border border-border bg-hover px-2.5 py-0.5 text-fg"
                    >
                      {c.replace(/_/g, ' ')}: <b>{n}</b>
                    </span>
                  ))}
                </div>
              </div>
            )}

            {!!s.preview.preview?.length && (
              <div className="max-h-48 overflow-auto rounded-lg border border-border">
                <table className="w-full text-left text-xs" aria-label={t('import.itemsToImport')}>
                  <thead className="sticky top-0 bg-panel text-muted">
                    <tr>
                      <th className="p-1.5">{t('import.colTitle')}</th>
                      <th className="p-1.5">{t('import.colType')}</th>
                      <th className="p-1.5">{t('import.colUsername')}</th>
                      <th className="p-1.5">{t('import.colDetails')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {s.preview.preview.map((it, i) => (
                      <tr key={i} className="border-t border-border">
                        <td className="p-1.5 font-medium">
                          {it.title}
                          {it.archived ? ` (${t('import.archivedTag')})` : ''}
                        </td>
                        <td className="p-1.5">{it.category.replace(/_/g, ' ')}</td>
                        <td className="p-1.5">{it.username ?? ''}</td>
                        <td className="p-1.5 text-muted">
                          {[
                            it.hasPassword && t('import.passwordTag'),
                            it.hasTotp && t('import.totpTag'),
                            it.fields ? t('import.fieldsTag', { n: it.fields }) : '',
                            ...it.tags.map((tg) => `#${tg}`),
                          ]
                            .filter(Boolean)
                            .join(' · ')}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {!!s.preview.duplicateTitles?.length && (
              <p className="text-xs text-muted">
                {t('import.duplicatesSkippedList', {
                  titles: s.preview.duplicateTitles.join(', '),
                })}
              </p>
            )}

            {!!s.preview.warnings?.length && (
              <div
                className="rounded-lg border border-warn/30 bg-warn/10 p-2.5 text-xs text-warn"
                aria-label={t('import.warnings')}
              >
                <p className="font-semibold">{t('import.warnings')}</p>
                <ul className="mt-1 list-disc pl-4 space-y-0.5">
                  {s.preview.warnings.map((w, i) => (
                    <li key={i}>{w}</li>
                  ))}
                </ul>
              </div>
            )}
          </div>

          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => dispatch({ type: 'back' })}>
              {t('import.back')}
            </Button>
            <Button variant="default" disabled={busy} onClick={() => run(false)}>
              {busy ? t('import.importing') : t('import.confirm')}
            </Button>
          </div>
        </div>
      )}

      {s.step === 'done' && s.result && (
        <div className="space-y-4 py-4 text-center">
          <CheckCircle2 className="mx-auto text-ok" size={48} />
          <h4 className="text-lg font-semibold text-fg">{t('import.doneTitle')}</h4>
          <p className="text-sm text-fg">
            {t('import.doneSummary', {
              added: s.result.added,
              parsed: s.result.parsed,
              duplicates: s.result.duplicates,
            })}
          </p>
          <p className="text-xs text-muted">{t('import.doneTip')}</p>
          <div className="flex justify-center gap-3 pt-2">
            <Button variant="default" onClick={handleViewImported}>
              {t('import.viewItems')}
            </Button>
            <Button variant="ghost" onClick={onDone}>
              {t('common.close')}
            </Button>
          </div>
        </div>
      )}

      {s.error && (
        <p role="alert" className="mt-3 text-sm text-danger">
          {s.error}
        </p>
      )}
    </div>
  );
}
