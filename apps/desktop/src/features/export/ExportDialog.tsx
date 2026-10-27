import { useEffect, useState } from 'react';
import { api } from '../../api/client';
import type { ExportRequest, ItemRef } from '../../api/types';
import { useT } from '../../i18n';
import { chooseExportPath } from '../../lib/saveDialog';
import { cn } from '../../lib/utils';
import { Button, Input, Modal, toast } from '../../ui';
import { errText } from '../items/actions';

export function ExportDialog({ open, onOpenChange, vaultId, vaultName, items }: {
  open: boolean; onOpenChange: (o: boolean) => void; vaultId?: string; vaultName?: string; items?: ItemRef[];
}) {
  const t = useT();
  const selected = items ?? [];
  const [format, setFormat] = useState<'json' | 'csv'>('json');
  const [pass, setPass] = useState(''); const [confirm, setConfirm] = useState('');
  const [ack, setAck] = useState(false); const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    if (open) { setFormat('json'); setPass(''); setConfirm(''); setAck(false); setBusy(false); setErr(null); }
  }, [open]);
  const encrypted = format === 'json';
  const passOk = pass.length >= 8 && pass === confirm;
  const valid = (!!vaultId || selected.length > 0) && (encrypted ? passOk : ack);
  const title = selected.length ? t('export.titleSelected') : t('export.titleVault');
  const scope = selected.length ? t('export.scopeSelected', { n: selected.length }) : t('export.scopeVault', { vault: vaultName ?? '' });
  const submit = async () => {
    if (!valid || busy) return;
    const ext = encrypted ? 'json' : 'csv';
    const name = `${selected.length ? 'gitpass-selected' : 'gitpass-export'}.${ext}`;
    let path: string | null;
    try { path = await chooseExportPath(name, ext); } catch (e) { setErr(errText(e)); return; }
    if (!path) return;
    setBusy(true); setErr(null);
    try {
      const req: ExportRequest = { format: encrypted ? 'gitpass-json' : 'csv', encrypted, path,
        ...(selected.length ? { items: selected } : vaultId ? { vaultId } : {}), ...(encrypted ? { passphrase: pass } : {}) };
      const r = await api.exportItems({ req });
      toast(t('export.done', { n: r.items, path: r.path }));
      onOpenChange(false);
    } catch (e) { setErr(errText(e)); } finally { setBusy(false); }
  };
  return <Modal open={open} onOpenChange={(o) => { if (!busy) onOpenChange(o); }} title={title} description={scope}
    footer={<><Button variant="outline" onClick={() => onOpenChange(false)} disabled={busy}>{t('common.cancel')}</Button>
      <Button variant="default" onClick={() => void submit()} disabled={!valid || busy}>{busy ? t('export.busy') : t('export.submit')}</Button></>}>
    <div className="space-y-4">
      <div role="radiogroup" aria-label={t('export.format')} className="grid gap-2">
        <label className={cn('flex cursor-pointer items-start gap-3 rounded-lg border p-3', encrypted ? 'border-accent bg-hover' : 'border-border')}>
          <input type="radio" name="export-format" className="mt-1 accent-accent" aria-label={t('export.json')} checked={encrypted} onChange={() => setFormat('json')} />
          <span><span className="block text-sm font-medium">{t('export.json')}</span><span className="mt-0.5 block text-xs text-muted">{t('export.jsonHelp')}</span></span>
        </label>
        <label className={cn('flex cursor-pointer items-start gap-3 rounded-lg border p-3', !encrypted ? 'border-accent bg-hover' : 'border-border')}>
          <input type="radio" name="export-format" className="mt-1 accent-accent" aria-label={t('export.csv')} checked={!encrypted} onChange={() => setFormat('csv')} />
          <span><span className="block text-sm font-medium">{t('export.csv')}</span><span className="mt-0.5 block text-xs text-muted">{t('export.csvHelp')}</span></span>
        </label>
      </div>
      {encrypted ? <div className="space-y-2">
        <label className="block text-xs text-muted">{t('export.passphrase')}<Input className="mt-1" type="password" autoComplete="new-password" spellCheck={false} value={pass} onChange={(e) => setPass(e.target.value)} /></label>
        <label className="block text-xs text-muted">{t('export.passphraseConfirm')}<Input className="mt-1" type="password" autoComplete="new-password" spellCheck={false} value={confirm} onChange={(e) => setConfirm(e.target.value)} /></label>
        {pass.length > 0 && pass.length < 8 && <p className="text-xs text-danger">{t('export.passphraseShort')}</p>}
        {confirm.length > 0 && pass !== confirm && <p className="text-xs text-danger">{t('export.passphraseMismatch')}</p>}
      </div> : <div className="rounded-lg border border-danger/40 bg-danger/10 p-3 text-xs text-danger">
        <p>{t('export.csvWarn')}</p>
        <label className="mt-2 flex items-start gap-2 text-fg"><input type="checkbox" className="mt-0.5 accent-accent" checked={ack} onChange={(e) => setAck(e.target.checked)} />{t('export.csvConfirm')}</label>
      </div>}
      {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    </div>
  </Modal>;
}
