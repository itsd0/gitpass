import { FormEvent, useEffect, useState } from 'react';
import { ChevronRight, Copy, Fingerprint, KeyRound, Loader2, PlusCircle, Printer } from 'lucide-react';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Button, Input, Logo } from '../ui';
import { EmergencyKitFlow } from '../features/onboarding/EmergencyKit';
import { useT } from '../i18n';

const msg = (x: unknown) => (x instanceof Error ? x.message : String(x));

export function Unlock() {
  const { unlock, setAccount } = useApp();
  const t = useT();
  const [vaultPath, setVaultPath] = useState('');
  const [pw, setPw] = useState('');
  const [pw2, setPw2] = useState('');
  const [sk, setSk] = useState('');
  const [showSk, setShowSk] = useState(false);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  const [exists, setExists] = useState(true);
  const [bio, setBio] = useState(false);
  const [mode, setMode] = useState<'unlock' | 'create'>('unlock');
  const [saveSk, setSaveSk] = useState(false);
  const [throttleLeft, setThrottleLeft] = useState<number>(0);
  const [created, setCreated] = useState<{ secretKey: string; account: string | null } | null>(null); const [showKit, setShowKit] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.status().then((s) => {
      setVaultPath((p) => p || s.vaultPath);
      if (s.unlocked) { setAccount(s.account); unlock(); return; }
      const ex = s.vaultExists ?? true; setExists(ex); if (!ex) setMode('create');
      setBio(!!s.biometricAvailable);
      if (s.throttledSecs && s.throttledSecs > 0) {
        setThrottleLeft(s.throttledSecs);
      }
    }).catch(() => {});
  }, [unlock, setAccount]);

  useEffect(() => {
    if (throttleLeft <= 0) return;
    const interval = setInterval(() => {
      setThrottleLeft((n) => Math.max(0, n - 1));
    }, 1000);
    return () => clearInterval(interval);
  }, [throttleLeft]);

  async function submit(e: FormEvent) {
    e.preventDefault(); setBusy(true); setErr('');
    try {
      const s = await api.unlock({ vaultPath, password: pw, secretKey: sk || null });
      setAccount(s.account); unlock();
    } catch (x) {
      const m = msg(x);
      const throttleMatch = m.match(/throttled.*?(\d+)\s*s?/i);
      if (throttleMatch) {
        setThrottleLeft(parseInt(throttleMatch[1], 10));
      } else {
        setErr(m);
        if (/not a gitpass vault/i.test(m)) { setExists(false); }
      }
    } finally { setPw(''); setSk(''); setBusy(false); }
  }
  async function touchId() {
    setBusy(true); setErr('');
    try { const s = await api.unlockBiometric({ vaultPath }); setAccount(s.account); unlock(); }
    catch (x) { setErr(msg(x)); } finally { setBusy(false); }
  }
  async function create(e: FormEvent) {
    e.preventDefault(); setErr('');
    if (pw.length < 8) { setErr(t('unlock.errShortPassword')); return; }
    if (pw !== pw2) { setErr(t('unlock.errPasswordMismatch')); return; }
    setBusy(true);
    try {
      const r = await api.createAccount({ vaultPath, password: pw, saveSecretKey: saveSk });
      const s = await api.status();
      setCreated({ secretKey: r.secretKey, account: s.account });
    } catch (x) { setErr(msg(x)); } finally { setPw(''); setPw2(''); setBusy(false); }
  }

  if (created) return (
    <div className="flex h-full items-center justify-center bg-bg" data-testid="secret-key-once">
      <div className="w-[440px] rounded-2xl border border-border bg-panel p-8 shadow-2xl">
        <div className="mb-4 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">{t('unlock.saveSecretKey')}</h1>
          <p className="text-center text-xs text-muted">{t('unlock.saveSecretKeyDesc')}</p></div>
        <div className="flex items-center gap-2 rounded-md border border-border bg-bg p-3 font-mono text-sm break-all">
          <span className="flex-1 select-all">{created.secretKey}</span>
          <Button variant="ghost" size="iconSm" aria-label={t('unlock.copySecretKey')} onClick={() => void navigator.clipboard?.writeText(created.secretKey)}><Copy size={14} /></Button>
        </div>
        <Button variant="outline" className="mt-3 w-full" onClick={() => setShowKit(true)}><Printer size={14} /> {t('unlock.printEmergencyKit')}</Button>
        {showKit && <EmergencyKitFlow onClose={() => setShowKit(false)} onPrinted={() => setSaved(true)} />}
        <label className="mt-4 flex items-center gap-2 text-sm"><input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} />{t('unlock.savedConfirmation')}</label>
        <Button size="lg" className="mt-6 w-full" disabled={!saved} onClick={() => { setAccount(created.account); setCreated(null); unlock(); }}>{t('unlock.continue')}</Button>
      </div>
    </div>
  );

  if (mode === 'create') return (
    <div className="flex h-full items-center justify-center bg-bg" data-testid="create-vault">
      <form onSubmit={create} className="w-[380px] rounded-2xl border border-border bg-panel p-8 shadow-2xl">
        <div className="mb-6 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">{t('unlock.createTitle')}</h1><p className="text-center text-xs text-muted">{exists ? (vaultPath || '…') : t('unlock.noVaultFound', { path: vaultPath || '…' })}</p></div>
        <label className="mb-1 block text-xs text-muted" htmlFor="npw">{t('unlock.masterPassword')}</label>
        <Input id="npw" type="password" autoFocus autoComplete="new-password" className="h-10" value={pw} onChange={(e) => setPw(e.target.value)} placeholder={t('unlock.newPasswordPlaceholder')} />
        <label className="mb-1 mt-3 block text-xs text-muted" htmlFor="npw2">{t('unlock.confirmPassword')}</label>
        <Input id="npw2" type="password" autoComplete="new-password" className="h-10" value={pw2} onChange={(e) => setPw2(e.target.value)} placeholder={t('unlock.confirmPasswordPlaceholder')} />
        <Input aria-label={t('unlock.vaultFolder')} className="mt-3" value={vaultPath} onChange={(e) => setVaultPath(e.target.value)} placeholder="~/.gitpass" />
        <label className="mt-3 flex items-center gap-2 text-xs text-muted"><input type="checkbox" checked={saveSk} onChange={(e) => setSaveSk(e.target.checked)} />{t('unlock.storeSecretKeyLocal')}</label>
        {err && <p className="mt-2 text-xs text-danger" role="alert">{err}</p>}
        <Button type="submit" size="lg" className="mt-6 w-full" disabled={busy || !pw || !pw2}>{busy ? <><Loader2 size={15} className="animate-spin" /> {t('unlock.creating')}</> : <><PlusCircle size={15} /> {t('unlock.createVault')}</>}</Button>
        {exists && <button type="button" onClick={() => { setMode('unlock'); setErr(''); }} className="mt-4 w-full text-center text-xs text-accent hover:underline">{t('unlock.backToUnlock')}</button>}
      </form>
    </div>
  );

  return (
    <div className="flex h-full items-center justify-center bg-bg" data-testid="unlock">
      <form onSubmit={submit} className="w-[380px] rounded-2xl border border-border bg-panel p-8 shadow-2xl">
        <div className="mb-6 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">{t('unlock.title')}</h1><p className="text-center text-xs text-muted">{vaultPath || '…'}</p></div>
        <label className="mb-1 block text-xs text-muted" htmlFor="pw">{t('unlock.masterPassword')}</label>
        <div className="relative">
          <Input id="pw" type="password" autoFocus autoComplete="off" className="h-10 pr-10" value={pw} onChange={(e) => setPw(e.target.value)} placeholder={t('unlock.masterPasswordPlaceholder')} />
          <button type="submit" disabled={busy || !pw || throttleLeft > 0} aria-label={t('unlock.submit')} className="absolute right-1 top-1 flex h-8 w-8 items-center justify-center rounded-md bg-accent text-white disabled:opacity-40">{busy ? <Loader2 size={15} className="animate-spin" /> : <ChevronRight size={16} />}</button>
        </div>
        {(throttleLeft > 0 ? t('unlock.throttled', { n: throttleLeft }) : err) && (
          <p className="mt-2 text-xs text-danger" role="alert">
            {throttleLeft > 0 ? t('unlock.throttled', { n: throttleLeft }) : err}
          </p>
        )}
        {!exists && <Button type="button" variant="outline" className="mt-3 w-full" onClick={() => { setMode('create'); setErr(''); }}><PlusCircle size={14} /> {t('unlock.noVaultHere')}</Button>}
        <button type="button" onClick={() => setShowSk(!showSk)} className="mt-4 flex items-center gap-1.5 text-xs text-accent hover:underline"><KeyRound size={12} />{showSk ? t('unlock.hideAdvanced') : t('unlock.showAdvanced')}</button>
        {showSk && <div className="mt-3 space-y-2">
          <Input aria-label={t('unlock.vaultFolder')} value={vaultPath} onChange={(e) => setVaultPath(e.target.value)} placeholder="~/.gitpass" />
          <Input aria-label={t('unlock.secretKey')} type="password" autoComplete="off" value={sk} onChange={(e) => setSk(e.target.value)} placeholder={t('unlock.secretKeyPlaceholder')} />
        </div>}
        <Button type="submit" size="lg" className="mt-6 w-full" disabled={busy || !pw || throttleLeft > 0}>{busy ? t('unlock.unlocking') : t('unlock.submit')}</Button>
        {bio && <Button type="button" variant="outline" size="lg" className="mt-2 w-full" disabled={busy} onClick={() => void touchId()}><Fingerprint size={16} /> {t('unlock.touchId')}</Button>}
      </form>
    </div>
  );
}
