import { useEffect, useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import { api } from '../../api/client';
import type { Member, VaultInfo } from '../../api/types';
import { Button, Modal, Input, toast } from '../../ui';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { looksLikePublicKey, verificationGroups } from './verify';

export function ShareDialog({ vault, open, onOpenChange, onShared }: { vault: VaultInfo; open: boolean; onOpenChange: (o: boolean) => void; onShared?: (m: Member) => void }) {
  const t = useT();
  const [key, setKey] = useState(''); const [label, setLabel] = useState(''); const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const shaped = looksLikePublicKey(key);
  // Real fingerprint from Rust; the share button stays disabled until it is known AND ticked.
  const [fpr, setFpr] = useState<string | null>(null); const [fprErr, setFprErr] = useState(false);
  useEffect(() => { setFpr(null); setFprErr(false); if (!shaped) return; let live = true;
    api.fingerprintOf({ publicKey: key.trim() }).then((f) => live && setFpr(f), () => live && setFprErr(true));
    return () => { live = false; }; }, [key, shaped]);
  const valid = shaped && !fprErr;
  const close = (o: boolean) => { if (!o) { setKey(''); setLabel(''); setConfirmed(false); setErr(null); } onOpenChange(o); };
  const submit = async () => {
    if (!valid || !fpr || !confirmed) return; setBusy(true); setErr(null);
    try { const m = await api.shareVault({ vaultId: vault.id, publicKey: key.trim(), label: label.trim() || null }); toast(t('share.done', { name: m.label ?? m.fingerprint })); onShared?.(m); close(false); }
    catch (e) { setErr(String((e as Error)?.message ?? e)); } finally { setBusy(false); }
  };
  return (
    <Modal open={open} onOpenChange={close} title={t('share.title', { vault: vault.name })} description={t('share.desc')} wide
      footer={<><Button variant="outline" onClick={() => close(false)}>{t('common.cancel')}</Button>
        <Button variant="default" disabled={!valid || !fpr || !confirmed || busy} onClick={submit} data-testid="share-submit">{t('share.submit')}</Button></>}>
      <div className="space-y-4">
        <label className="block text-sm">
          <span className="mb-1 block text-muted">{t('share.key')}</span>
          <div className="flex gap-2">
            <Input value={key} onChange={(e) => { setKey(e.target.value); setConfirmed(false); }} placeholder={t('share.keyPlaceholder')} aria-invalid={!!key && !valid} className="font-mono" />
            <Button variant="outline" type="button" onClick={async () => { try { setKey((await navigator.clipboard.readText()).trim()); setConfirmed(false); } catch { /* clipboard denied */ } }}>{t('share.pasteClipboard')}</Button>
          </div>
          {key && !valid && <span role="alert" className="mt-1 block text-xs text-danger">{t('share.invalid')}</span>}
        </label>
        <label className="block text-sm"><span className="mb-1 block text-muted">{t('share.label')}</span>
          <Input value={label} onChange={(e) => setLabel(e.target.value)} placeholder={t('share.labelPlaceholder')} /></label>
        {valid && fpr && (
          <div className="rounded-lg border border-warn/40 bg-warn/5 p-3" data-testid="verify-box">
            <div className="flex items-center gap-2 text-sm font-medium"><ShieldCheck className="h-4 w-4 text-warn" aria-hidden />{t('share.verify')}</div>
            <p className="mt-1 text-xs text-muted">{t('share.verifyHelp')}</p>
            <div className="mt-2 flex flex-wrap gap-1.5 font-mono text-sm" aria-label="verification code" data-testid="recipient-fingerprint">{verificationGroups(fpr).map((g, i) => <span key={i} className="rounded bg-card px-1.5 py-0.5">{g}</span>)}</div>
            <label className="mt-3 flex items-start gap-2 text-sm">
              <input type="checkbox" className="mt-0.5 h-4 w-4 accent-[rgb(var(--accent))]" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
              <span>{t('share.confirm')}</span></label>
          </div>)}
        {err && <p role="alert" className="text-sm text-danger">{t('common.error', { msg: err })}</p>}
      </div>
    </Modal>);
}

/** Registry modal for `openModal('people', { vaultId })` (ItemDetail → Share). */
export function ShareDialogHost() {
  const props = useApp((s) => s.modal?.props) as { vaultId?: string } | undefined; const closeModal = useApp((s) => s.closeModal);
  const [vault, setVault] = useState<VaultInfo | null>(null);
  useEffect(() => { let live = true; api.listVaults().then((vs) => { if (!live) return; const v = vs.find((x) => x.id === props?.vaultId) ?? null; if (v) setVault(v); else { toast('Vault not found', 'danger'); closeModal(); } }, (e) => { toast(String(e), 'danger'); closeModal(); });
    return () => { live = false; }; }, [props?.vaultId, closeModal]);
  return vault ? <ShareDialog vault={vault} open onOpenChange={(o) => !o && closeModal()} onShared={() => useApp.getState().refreshItems()} /> : null;
}
