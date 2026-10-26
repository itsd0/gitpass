import { FormEvent, useEffect, useState } from 'react';
import { ChevronRight, KeyRound, Loader2 } from 'lucide-react';
import { api } from '../api/client';
import { useApp } from '../lib/store';
import { Button, Input, Logo } from '../ui';

export function Unlock() {
  const { unlock, setAccount } = useApp();
  const [vaultPath, setVaultPath] = useState('');
  const [pw, setPw] = useState('');
  const [sk, setSk] = useState('');
  const [showSk, setShowSk] = useState(false);
  const [err, setErr] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => { api.status().then((s) => { setVaultPath((p) => p || s.vaultPath); if (s.unlocked) { setAccount(s.account); unlock(); } }).catch(() => {}); }, [unlock, setAccount]);

  async function submit(e: FormEvent) {
    e.preventDefault(); setBusy(true); setErr('');
    try {
      const s = await api.unlock({ vaultPath, password: pw, secretKey: sk || null });
      setAccount(s.account); unlock();
    } catch (x) { setErr(x instanceof Error ? x.message : String(x)); } finally { setPw(''); setSk(''); setBusy(false); }
  }
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
        <button type="button" onClick={() => setShowSk(!showSk)} className="mt-4 flex items-center gap-1.5 text-xs text-accent hover:underline"><KeyRound size={12} />{showSk ? 'Hide advanced' : 'Vault folder & Secret Key'}</button>
        {showSk && <div className="mt-3 space-y-2">
          <Input aria-label="Vault folder" value={vaultPath} onChange={(e) => setVaultPath(e.target.value)} placeholder="~/.gitpass" />
          <Input aria-label="Secret Key" type="password" autoComplete="off" value={sk} onChange={(e) => setSk(e.target.value)} placeholder="Secret Key (A3-XXXXXX-…), optional" />
        </div>}
        <Button type="submit" size="lg" className="mt-6 w-full" disabled={busy || !pw}>{busy ? 'Unlocking…' : 'Unlock'}</Button>
      </form>
    </div>
  );
}
