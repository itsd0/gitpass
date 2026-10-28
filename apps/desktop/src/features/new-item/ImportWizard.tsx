import { useEffect, useReducer, useState } from 'react';
import { ArrowLeft, CheckCircle2, FileUp } from 'lucide-react';
import type { VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { Button, Input } from '../../ui';
import { useT } from '../../i18n';
import { IMPORT_FORMATS, canPreview, importReducer, initialImport } from './importWizardLogic';

export function ImportWizard({ onBack, onDone }: { onBack: () => void; onDone: () => void }) {
  const t = useT();
  const [s, dispatch] = useReducer(importReducer, initialImport());
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .listVaults()
      .then((v) => {
        setVaults(v);
        if (v[0]) dispatch({ type: 'vault', vaultId: v[0].id });
      })
      .catch((e) => dispatch({ type: 'error', error: String(e) }));
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
      });
      dispatch(dryRun ? { type: 'preview', preview: r } : { type: 'done', result: r });
      if (!dryRun) api.setOnboarding({ patch: { imported: true } }).catch(() => {});
    } catch (e) {
      dispatch({ type: 'error', error: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const stepIdx = ['format', 'file', 'preview', 'done'].indexOf(s.step);
  const selectedFormat = IMPORT_FORMATS.find((f) => f.id === s.format);

  return (
    <div aria-label="Import wizard">
      <div className="mb-4 flex items-center gap-2">
        <Button
          size="icon"
          variant="ghost"
          aria-label="Back"
          onClick={() => (s.step === 'format' ? onBack() : dispatch({ type: 'back' }))}
        >
          <ArrowLeft size={16} />
        </Button>
        <h3 className="text-lg font-semibold">{t('import.title')}</h3>
        <span className="ml-auto text-xs text-muted">Step {stepIdx + 1} of 4</span>
      </div>

      {s.step === 'format' && (
        <div className="grid grid-cols-2 gap-2">
          {IMPORT_FORMATS.map((f) => (
            <button
              key={f.id}
              className="flex flex-col justify-start rounded-lg border border-border bg-hover p-3 text-left text-sm hover:border-accent"
              onClick={() => dispatch({ type: 'format', format: f.id })}
            >
              <span className="font-medium text-fg">{t(f.labelKey)}</span>
              {f.hintKey && <span className="mt-1 text-xs text-muted leading-tight">{t(f.hintKey)}</span>}
            </button>
          ))}
        </div>
      )}

      {s.step === 'file' && (
        <div className="space-y-3">
          <p className="text-sm text-muted">
            Format: <b className="text-fg">{selectedFormat ? t(selectedFormat.labelKey) : ''}</b>. Export file from your old
            manager, then enter its path. The file is read locally by gitpass; nothing is uploaded.
          </p>
          {selectedFormat?.hintKey && (
            <div className="rounded-md border border-border bg-hover/50 px-3 py-2 text-xs text-muted">
              {t(selectedFormat.hintKey)}
            </div>
          )}
          <label className="block text-xs text-muted">
            Export file path
            <Input
              className="mt-1"
              aria-label="Export file path"
              placeholder={`/Users/you/Downloads/export.${selectedFormat?.ext ?? 'csv'}`}
              value={s.path}
              onChange={(e) => dispatch({ type: 'path', path: e.target.value })}
            />
          </label>
          <label className="block text-xs text-muted">
            Import into vault
            <select
              aria-label="Import vault"
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
          <div className="flex justify-end">
            <Button variant="default" disabled={!canPreview(s) || busy} onClick={() => run(true)}>
              <FileUp size={14} /> {busy ? 'Checking…' : 'Preview import'}
            </Button>
          </div>
        </div>
      )}

      {s.step === 'preview' && s.preview && (
        <div className="space-y-4">
          <p className="text-sm text-muted">Dry run complete — nothing has been written yet.</p>
          <dl className="grid grid-cols-3 gap-2 text-center">
            {(
              [
                ['Parsed', s.preview.parsed],
                ['Will add', s.preview.parsed - s.preview.duplicates],
                ['Duplicates skipped', s.preview.duplicates],
              ] as const
            ).map(([k, v]) => (
              <div key={k} className="rounded-lg bg-hover p-3">
                <dt className="text-xs text-muted">{k}</dt>
                <dd className="text-2xl font-semibold" data-testid={`preview-${k}`}>
                  {v}
                </dd>
              </div>
            ))}
          </dl>
          {s.preview.categories && Object.keys(s.preview.categories).length > 0 && (
            <div className="flex flex-wrap gap-1.5 text-xs" aria-label="Item types">
              {Object.entries(s.preview.categories).map(([c, n]) => (
                <span key={c} className="rounded-full bg-hover px-2 py-0.5">
                  {c.replace(/_/g, ' ')}: <b>{n}</b>
                </span>
              ))}
            </div>
          )}
          {!!s.preview.preview?.length && (
            <div className="max-h-48 overflow-auto rounded-lg border border-border">
              <table className="w-full text-left text-xs" aria-label="Items to import">
                <thead className="sticky top-0 bg-panel text-muted">
                  <tr>
                    <th className="p-1.5">Title</th>
                    <th className="p-1.5">Type</th>
                    <th className="p-1.5">Username</th>
                    <th className="p-1.5">Details</th>
                  </tr>
                </thead>
                <tbody>
                  {s.preview.preview.map((it, i) => (
                    <tr key={i} className="border-t border-border">
                      <td className="p-1.5 font-medium">
                        {it.title}
                        {it.archived ? ' (archived)' : ''}
                      </td>
                      <td className="p-1.5">{it.category.replace(/_/g, ' ')}</td>
                      <td className="p-1.5">{it.username ?? ''}</td>
                      <td className="p-1.5 text-muted">
                        {[
                          it.hasPassword && 'password',
                          it.hasTotp && 'one-time code',
                          it.fields ? `${it.fields} fields` : '',
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
            <p className="text-xs text-muted">Duplicates skipped: {s.preview.duplicateTitles.join(', ')}</p>
          )}
          {!!s.preview.warnings?.length && (
            <ul className="list-disc pl-4 text-xs text-warn" aria-label="Import warnings">
              {s.preview.warnings.map((w, i) => (
                <li key={i}>{w}</li>
              ))}
            </ul>
          )}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => dispatch({ type: 'back' })}>
              Back
            </Button>
            <Button variant="default" disabled={busy} onClick={() => run(false)}>
              {busy ? 'Importing…' : 'Confirm import'}
            </Button>
          </div>
        </div>
      )}

      {s.step === 'done' && s.result && (
        <div className="space-y-3 text-center">
          <CheckCircle2 className="mx-auto text-ok" size={40} />
          <p>
            Imported <b>{s.result.added}</b> of {s.result.parsed} items ({s.result.duplicates} duplicates skipped).
          </p>
          <p className="text-xs text-muted">Delete the unencrypted export file from your disk.</p>
          <Button variant="default" onClick={onDone}>
            Done
          </Button>
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
