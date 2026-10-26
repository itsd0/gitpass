import { FormEvent, useEffect, useState } from 'react';
import { ChevronRight, Copy, Fingerprint, KeyRound, Loader2, PlusCircle, Printer } from 'lucide-react';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Button, Input, Logo } from '../ui';
import { EmergencyKitFlow } from '../features/onboarding/EmergencyKit';

const msg = (x: unknown) => (x instanceof Error ? x.message : String(x));

export function Unlock() {
  const { unlock, setAccount } = useApp();
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
  const [created, setCreated] = useState<{ secretKey: string; account: string | null } | null>(null); const [showKit, setShowKit] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.status().then((s) => {
      setVaultPath((p) => p || s.vaultPath);
      if (s.unlocked) { setAccount(s.account); unlock(); return; }
      const ex = s.vaultExists ?? true; setExists(ex); if (!ex) setMode('create');
      setBio(!!s.biometricAvailable);
    }).catch(() => {});
  }, [unlock, setAccount]);

  async function submit(e: FormEvent) {
    e.preventDefault(); setBusy(true); setErr('');
    try {
      const s = await api.unlock({ vaultPath, password: pw, secretKey: sk || null });
      setAccount(s.account); unlock();
    } catch (x) {
      const m = msg(x); setErr(m);
      if (/not a gitpass vault/i.test(m)) { setExists(false); }
    } finally { setPw(''); setSk(''); setBusy(false); }
  }
  async function touchId() {
    setBusy(true); setErr('');
    try { const s = await api.unlockBiometric({ vaultPath }); setAccount(s.account); unlock(); }
    catch (x) { setErr(msg(x)); } finally { setBusy(false); }
  }
  async function create(e: FormEvent) {
    e.preventDefault(); setErr('');
    if (pw.length < 8) { setErr('Master password must be at least 8 characters.'); return; }
    if (pw !== pw2) { setErr('Passwords do not match.'); return; }
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
        <div className="mb-4 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">Save your Secret Key</h1>
          <p className="text-center text-xs text-muted">You need it together with your master password to unlock on a new device. It is shown only once.</p></div>
        <div className="flex items-center gap-2 rounded-md border border-border bg-bg p-3 font-mono text-sm break-all">
          <span className="flex-1 select-all">{created.secretKey}</span>
          <Button variant="ghost" size="iconSm" aria-label="Copy Secret Key" onClick={() => void navigator.clipboard?.writeText(created.secretKey)}><Copy size={14} /></Button>
        </div>
        <Button variant="outline" className="mt-3 w-full" onClick={() => setShowKit(true)}><Printer size={14} /> Print Emergency Kit</Button>
        {showKit && <EmergencyKitFlow onClose={() => setShowKit(false)} onPrinted={() => setSaved(true)} />}
        <label className="mt-4 flex items-center gap-2 text-sm"><input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} />I saved my Secret Key somewhere safe</label>
        <Button size="lg" className="mt-6 w-full" disabled={!saved} onClick={() => { setAccount(created.account); setCreated(null); unlock(); }}>Continue</Button>
      </div>
    </div>
  );

  if (mode === 'create') return (
    <div className="flex h-full items-center justify-center bg-bg" data-testid="create-vault">
      <form onSubmit={create} className="w-[380px] rounded-2xl border border-border bg-panel p-8 shadow-2xl">
        <div className="mb-6 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">Create a gitpass vault</h1><p className="text-center text-xs text-muted">{exists ? '' : 'No vault found at '}{vaultPath || '…'}</p></div>
        <label className="mb-1 block text-xs text-muted" htmlFor="npw">Master password</label>
        <Input id="npw" type="password" autoFocus autoComplete="new-password" className="h-10" value={pw} onChange={(e) => setPw(e.target.value)} placeholder="At least 8 characters" />
        <label className="mb-1 mt-3 block text-xs text-muted" htmlFor="npw2">Confirm master password</label>
        <Input id="npw2" type="password" autoComplete="new-password" className="h-10" value={pw2} onChange={(e) => setPw2(e.target.value)} placeholder="Repeat password" />
        <Input aria-label="Vault folder" className="mt-3" value={vaultPath} onChange={(e) => setVaultPath(e.target.value)} placeholder="~/.gitpass" />
        <label className="mt-3 flex items-center gap-2 text-xs text-muted"><input type="checkbox" checked={saveSk} onChange={(e) => setSaveSk(e.target.checked)} />Also store the Secret Key in the vault folder on this Mac (secret.key, 0600)</label>
        {err && <p className="mt-2 text-xs text-danger" role="alert">{err}</p>}
        <Button type="submit" size="lg" className="mt-6 w-full" disabled={busy || !pw || !pw2}>{busy ? <><Loader2 size={15} className="animate-spin" /> Creating…</> : <><PlusCircle size={15} /> Create vault</>}</Button>
        {exists && <button type="button" onClick={() => { setMode('unlock'); setErr(''); }} className="mt-4 w-full text-center text-xs text-accent hover:underline">Back to unlock</button>}
      </form>
    </div>
  );

  return (
    <div className="flex h-full items-center justify-center bg-bg" data-testid="unlock">
      <form onSubmit={submit} className="w-[380px] rounded-2xl border border-border bg-panel p-8 shadow-2xl">
        <div className="mb-6 flex flex-col items-center gap-3"><Logo size={56} /><h1 className="text-xl font-semibold">Unlock gitpass</h1><p className="text-center text-xs text-muted">{vaultPath || '…'}</p></div>
        <label className="mb-1 block text-xs text-muted" htmlFor="pw">Master password</label>
        <div className="relative">
          <Input id="pw" type="password" autoFocus autoComplete="off" className="h-10 pr-10" value={pw} onChange={(e) => setPw(e.target.value)} placeholder="Enter your password" />
          <button type="submit" disabled={busy || !pw} aria-label="Unlock" className="absolute right-1 top-1 flex h-8 w-8 items-center justify-center rounded-md bg-accent text-white disabled:opacity-40">{busy ? <Loader2 size={15} className="animate-spin" /> : <ChevronRight size={16} />}</button>
        </div>
        {err && <p className="mt-2 text-xs text-danger" role="alert">{err}</p>}
        {!exists && <Button type="button" variant="outline" className="mt-3 w-full" onClick={() => { setMode('create'); setErr(''); }}><PlusCircle size={14} /> Create a new vault here</Button>}
        <button type="button" onClick={() => setShowSk(!showSk)} className="mt-4 flex items-center gap-1.5 text-xs text-accent hover:underline"><KeyRound size={12} />{showSk ? 'Hide advanced' : 'Vault folder & Secret Key'}</button>
        {showSk && <div className="mt-3 space-y-2">
          <Input aria-label="Vault folder" value={vaultPath} onChange={(e) => setVaultPath(e.target.value)} placeholder="~/.gitpass" />
          <Input aria-label="Secret Key" type="password" autoComplete="off" value={sk} onChange={(e) => setSk(e.target.value)} placeholder="Secret Key (A3-XXXXXX-…), optional" />
        </div>}
        <Button type="submit" size="lg" className="mt-6 w-full" disabled={busy || !pw}>{busy ? 'Unlocking…' : 'Unlock'}</Button>
        {bio && <Button type="button" variant="outline" size="lg" className="mt-2 w-full" disabled={busy} onClick={() => void touchId()}><Fingerprint size={16} /> Unlock with Touch ID</Button>}
      </form>
    </div>
  );
}
