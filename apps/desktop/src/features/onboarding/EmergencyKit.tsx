import { FormEvent, useCallback, useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Check, ClipboardCopy, KeyRound, Loader2, Printer, RefreshCw, X } from 'lucide-react';
import logo from '../../assets/logo.png';
import type { EmergencyKitData } from '../../api/types';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { toast } from '../../ui';

/** JSON payload encoded in the kit QR code. The Secret Key is only included when it is shown on the kit. */
export function kitQrPayload(d: EmergencyKitData, includeSecret: boolean): string {
  const o: Record<string, string> = { app: 'gitpass', v: '1', account: d.accountId, vault: d.vaultPath };
  if (d.remote) o.remote = d.remote;
  if (includeSecret && d.secretKey) o.secretKey = d.secretKey;
  return JSON.stringify(o);
}

export function formatKitDate(iso: string): string {
  const t = Date.parse(iso); if (Number.isNaN(t)) return iso;
  return new Date(t).toISOString().slice(0, 10);
}

/** Plain-text copy of the kit (fallback where the webview cannot print, e.g. macOS). */
export function kitText(d: EmergencyKitData, t: ReturnType<typeof useT>): string {
  return [t('kit.title'), '', `${t('kit.created')}: ${formatKitDate(d.createdAt)}`, `${t('kit.accountId')}: ${d.accountId}`,
    `${t('kit.vaultFolder')}: ${d.vaultPath}`, `${t('kit.remote')}: ${d.remote ?? t('kit.noRemote')}`, `${t('kit.secretKey')}: ${d.secretKey ?? ''}`].join('\n');
}

/** The printable A4 kit sheet (always white; print CSS in styles.css hides everything else). */
export function KitSheet({ data, onRotate, onReveal }: { data: EmergencyKitData; onRotate?: () => void; onReveal?: () => void }) {
  const t = useT(); const [qr, setQr] = useState<string | null>(null);
  const payload = kitQrPayload(data, true);
  useEffect(() => { let live = true; QRCode.toDataURL(payload, { margin: 1, width: 240, errorCorrectionLevel: 'M', color: { dark: '#111111', light: '#ffffff' } })
    .then(u => { if (live) setQr(u); }).catch(() => { if (live) setQr(null); }); return () => { live = false; }; }, [payload]);
  const sk = data.secretKey;
  return (
    <article className="ek-sheet mx-auto bg-white text-[#111]" data-testid="emergency-kit-sheet">
      <header className="flex items-center gap-4 border-b-4 border-[#f97316] pb-5">
        <img src={logo} alt="" className="h-14 w-14" />
        <div className="flex-1"><h1 className="text-[26px] font-bold leading-tight">{t('kit.title')}</h1>
          <p className="text-[13px] text-[#555]">{t('kit.subtitle')}</p></div>
        <dl className="text-right text-[12px] leading-5 text-[#333]">
          <div><dt className="inline text-[#777]">{t('kit.created')}: </dt><dd className="inline font-medium" data-testid="kit-date">{formatKitDate(data.createdAt)}</dd></div>
          {data.deviceName && <div><dt className="inline text-[#777]">{t('kit.device')}: </dt><dd className="inline font-medium">{data.deviceName}</dd></div>}
        </dl>
      </header>
      <section className="mt-6 grid grid-cols-[1fr_auto] gap-6">
        <div className="space-y-4">
          <KitField label={t('kit.accountId')} value={data.accountId} mono />
          <KitField label={t('kit.vaultFolder')} value={data.vaultPath} mono />
          <KitField label={t('kit.remote')} value={data.remote ?? t('kit.noRemote')} mono={!!data.remote} />
        </div>
        <figure className="flex w-[150px] flex-col items-center">
          {qr ? <img src={qr} alt={t('kit.qrAlt')} className="h-[140px] w-[140px]" data-testid="kit-qr" /> : <div className="h-[140px] w-[140px] border border-dashed border-[#bbb]" />}
          <figcaption className="mt-1 text-center text-[10px] leading-3 text-[#666]">{sk ? t('kit.qrWithKey') : t('kit.qrNoKey')}</figcaption>
        </figure>
      </section>
      <section className="mt-6">
        <h2 className="ek-label">{t('kit.secretKey')}</h2>
        <div className="mt-1 flex min-h-[64px] items-center rounded-lg border-2 border-[#f97316] bg-[#fff7ed] px-4 py-3" data-testid="kit-secret-key">
          {sk ? <span className="select-all break-all font-mono text-[18px] font-semibold tracking-wide">{sk}</span>
            : <div className="flex w-full flex-wrap items-center justify-between gap-3 text-[13px] text-[#9a3412]"><span className="ek-no-print">{t('kit.secretMissing')}</span>
              <span className="ek-no-print flex gap-2">
                {onReveal && <button type="button" className="ek-btn" onClick={onReveal}><KeyRound size={14} /> {t('kit.reveal')}</button>}
                {onRotate && <button type="button" className="ek-btn" onClick={onRotate}><RefreshCw size={14} /> {t('kit.rotateToPrint')}</button>}
              </span></div>}
        </div>
      </section>
      <section className="mt-6">
        <h2 className="ek-label">{t('kit.masterPassword')}</h2>
        <div className="mt-1 rounded-lg border-2 border-[#d4d4d4] px-4 pb-3 pt-10"><div className="border-b border-[#999]" /></div>
        <p className="mt-1 text-[11px] text-[#666]">{t('kit.masterHint')}</p>
      </section>
      <section className="mt-6">
        <h2 className="ek-label">{t('kit.howTo')}</h2>
        <ol className="mt-2 list-decimal space-y-1 pl-5 text-[13px] leading-5">
          <li>{t('kit.step1')}</li><li>{t('kit.step2')}</li><li>{t('kit.step3')}</li><li>{t('kit.step4')}</li>
        </ol>
      </section>
      <footer className="mt-8 border-t border-[#e5e5e5] pt-3 text-[11px] text-[#777]">{t('kit.footer')}</footer>
    </article>);
}

function KitField({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return <div><div className="ek-label">{label}</div><div className={`mt-1 break-all rounded-md border border-[#e5e5e5] bg-[#fafafa] px-3 py-2 text-[13px] ${mono ? 'font-mono' : ''}`}>{value}</div></div>;
}

/** Full-screen kit page with a toolbar (Print / Save as PDF, Close). */
export function EmergencyKitPage({ data, onClose, onPrinted, onReveal, onRotate, children }: {
  data: EmergencyKitData; onClose: () => void; onPrinted?: () => void; onReveal?: () => void; onRotate?: () => void; children?: React.ReactNode;
}) {
  const t = useT();
  return (<div className="ek-root fixed inset-0 z-50 overflow-auto bg-[#f4f4f5] text-black" role="dialog" aria-label={t('kit.title')}>
    <div className="ek-no-print sticky top-0 z-10 flex items-center justify-end gap-2 border-b border-[#e5e5e5] bg-white/95 p-3">
      <span className="mr-auto pl-2 text-sm text-[#555]">{t('kit.toolbarHint')}</span>
      <button type="button" className="ek-btn ek-btn-primary" onClick={() => { try { window.print(); } catch { /* some webviews cannot print */ } }}><Printer size={14} /> {t('kit.print')}</button>
      {data.secretKey && <button type="button" className="ek-btn" onClick={() => { navigator.clipboard.writeText(kitText(data, t)).then(() => toast(t('common.copied')), () => {}); }}><ClipboardCopy size={14} /> {t('kit.copyText')}</button>}
      {onPrinted && <button type="button" className="ek-btn" disabled={!data.secretKey} onClick={onPrinted}><Check size={14} /> {t('kit.markSaved')}</button>}
      <button type="button" className="ek-btn" onClick={onClose}><X size={14} /> {t('kit.close')}</button>
    </div>
    {children}
    <div className="py-8"><KitSheet data={data} onReveal={onReveal} onRotate={onRotate} /></div>
  </div>);
}

/** Loads the kit from the backend; Secret Key is revealed only after re-entering the master password, or via rotation. */
export function EmergencyKitFlow({ onClose, onPrinted }: { onClose: () => void; onPrinted?: () => void }) {
  const t = useT();
  const [data, setData] = useState<EmergencyKitData | null>(null); const [err, setErr] = useState<string | null>(null);
  const [ask, setAsk] = useState<null | 'reveal' | 'rotate'>(null); const [pw, setPw] = useState(''); const [busy, setBusy] = useState(false);
  const load = useCallback((password?: string) => api.emergencyKitData(password ? { password } : {}).then(d => { setData(d); setErr(null); }), []);
  useEffect(() => { load().catch(e => setErr(String(e))); }, [load]);
  async function submit(e: FormEvent) {
    e.preventDefault(); setBusy(true);
    try { if (ask === 'rotate') { await api.rotateSecretKey(); await load(); } else { await load(pw); } setAsk(null); }
    catch (x) { setErr(String(x)); } finally { setPw(''); setBusy(false); }
  }
  if (!data) return (<div className="ek-root fixed inset-0 z-50 flex items-center justify-center bg-white text-black" role="dialog" aria-label={t('kit.title')}>
    {err ? <div className="space-y-3 text-center"><p role="alert" className="text-red-600">{err}</p><button type="button" className="ek-btn" onClick={onClose}>{t('kit.close')}</button></div> : <Loader2 className="animate-spin" />}</div>);
  return (<EmergencyKitPage data={data} onClose={onClose} onPrinted={onPrinted} onReveal={() => { setErr(null); setAsk('reveal'); }} onRotate={() => { setErr(null); setAsk('rotate'); }}>
    {(ask || err) && <div className="ek-no-print mx-auto mt-4 max-w-[210mm] rounded-lg border border-[#e5e5e5] bg-white p-4 text-sm">
      {ask && <form onSubmit={submit} className="flex flex-wrap items-center gap-2" aria-label={ask === 'reveal' ? t('kit.reveal') : t('kit.rotateToPrint')}>
        <span className="w-full text-[#555]">{ask === 'reveal' ? t('kit.revealPrompt') : t('kit.rotatePrompt')}</span>
        {ask === 'reveal' && <input type="password" autoFocus autoComplete="off" aria-label={t('kit.masterPassword')} className="h-9 flex-1 rounded-md border border-[#ccc] px-2" value={pw} onChange={e => setPw(e.target.value)} />}
        <button type="submit" className="ek-btn ek-btn-primary" disabled={busy || (ask === 'reveal' && !pw)}>{busy ? <Loader2 size={14} className="animate-spin" /> : null}{ask === 'reveal' ? t('kit.reveal') : t('kit.rotateConfirm')}</button>
        <button type="button" className="ek-btn" onClick={() => { setAsk(null); setPw(''); }}>{t('common.cancel')}</button></form>}
      {err && <p role="alert" className="mt-2 text-red-600">{err}</p>}
    </div>}
  </EmergencyKitPage>);
}
