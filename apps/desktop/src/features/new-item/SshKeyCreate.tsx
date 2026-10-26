import { useEffect, useState } from 'react';
import { ArrowLeft, Copy } from 'lucide-react';
import type { SshKeyCreated, VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { Button, Input } from '../../ui';
import { useApp } from '../../lib/store';
import { CategoryIcon } from './CategoryIcon';

/** SSH Key creation: generate Ed25519 in the backend, or import an OpenSSH private key. The private key is never echoed back. */
export function SshKeyCreate({ vaultId: initialVault, initial, onBack, onDone }: { vaultId?: string; initial?: { title?: string }; onBack: () => void; onDone: () => void }) {
  const [vaults, setVaults] = useState<VaultInfo[]>([]); const [vaultId, setVaultId] = useState(initialVault ?? '');
  const [mode, setMode] = useState<'generate' | 'import'>('generate');
  const [title, setTitle] = useState(initial?.title ?? ''); const [comment, setComment] = useState('');
  const [pem, setPem] = useState(''); const [pass, setPass] = useState('');
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null); const [done, setDone] = useState<SshKeyCreated | null>(null);
  const { setRoute, selectItem } = useApp();
  useEffect(() => { api.listVaults().then(v => { setVaults(v); setVaultId(id => id || v[0]?.id || ''); }).catch(e => setErr(String(e))); }, []);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault(); if (!vaultId) { setErr('Choose a vault.'); return; }
    if (mode === 'import' && !pem.trim()) { setErr('Paste an OpenSSH private key.'); return; }
    setBusy(true); setErr(null);
    try {
      const r = mode === 'generate' ? await api.sshGenerate({ vaultId, title: title.trim() || 'SSH Key', comment: comment.trim() || null })
        : await api.sshImport({ vaultId, title: title.trim() || 'SSH Key', privateKey: pem, passphrase: pass || null });
      setPem(''); setPass(''); setDone(r);
    } catch (e) { setErr(String((e as Error)?.message ?? e)); } finally { setBusy(false); }
  };
  if (done) return (<div className="space-y-4" aria-label="SSH key created">
    <div className="flex items-center gap-3"><CategoryIcon category="ssh_key" size={40} /><h2 className="text-xl font-semibold">{done.item.title}</h2></div>
    <div><div className="mb-1 text-xs text-accent">public key</div>
      <div className="flex items-start gap-1"><code data-testid="ssh-public-key" className="block w-full break-all rounded-md border border-border bg-panel p-2 font-mono text-xs">{done.publicKey}</code>
        <Button type="button" size="icon" variant="ghost" aria-label="Copy public key" onClick={() => navigator.clipboard?.writeText(done.publicKey)}><Copy size={14} /></Button></div></div>
    <div><div className="mb-1 text-xs text-accent">fingerprint</div><code data-testid="ssh-fingerprint" className="font-mono text-xs">{done.fingerprint}</code> <span className="text-xs text-muted">({done.keyType})</span></div>
    <div className="flex justify-end"><Button onClick={() => { setRoute({ name: 'items', filter: { kind: 'vault', vaultId } }); selectItem(done.item.id); onDone(); }}>Done</Button></div></div>);
  return (<form onSubmit={submit} className="space-y-4" aria-label="New SSH Key">
    <div className="flex items-center gap-3">
      <Button type="button" size="icon" variant="ghost" aria-label="Back" onClick={onBack}><ArrowLeft size={16} /></Button>
      <CategoryIcon category="ssh_key" size={40} />
      <Input aria-label="Title" autoFocus className="h-11 border-transparent bg-transparent text-xl font-semibold" placeholder="SSH Key" value={title} onChange={e => setTitle(e.target.value)} /></div>
    <label className="block text-xs text-muted">Vault
      <select aria-label="Vault" className="mt-1 h-9 w-full rounded-md border border-border bg-panel px-2 text-sm text-fg" value={vaultId} onChange={e => setVaultId(e.target.value)}>{vaults.map(v => <option key={v.id} value={v.id}>{v.name}</option>)}</select></label>
    <div role="radiogroup" aria-label="Key source" className="flex rounded-lg border border-border p-0.5 text-sm">
      {(['generate', 'import'] as const).map(m => <button key={m} type="button" role="radio" aria-checked={mode === m} onClick={() => setMode(m)}
        className={'flex-1 rounded-md px-3 py-1 ' + (mode === m ? 'bg-accent text-white' : 'text-muted hover:text-fg')}>{m === 'generate' ? 'Generate Ed25519' : 'Import OpenSSH key'}</button>)}</div>
    {mode === 'generate' ? <div><div className="mb-1 text-xs text-accent">comment</div><Input aria-label="Comment" placeholder="you@laptop" value={comment} onChange={e => setComment(e.target.value)} /></div>
      : <><div><div className="mb-1 text-xs text-accent">private key</div>
          <textarea aria-label="Private key" rows={6} spellCheck={false} className="w-full rounded-md border border-border bg-panel p-2 font-mono text-xs outline-none focus:border-accent" placeholder="-----BEGIN OPENSSH PRIVATE KEY-----" value={pem} onChange={e => setPem(e.target.value)} /></div>
        <div><div className="mb-1 text-xs text-accent">passphrase (if encrypted)</div><Input aria-label="Passphrase" type="password" autoComplete="off" value={pass} onChange={e => setPass(e.target.value)} /></div></>}
    {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={onBack}>Cancel</Button><Button type="submit" disabled={busy}>{busy ? 'Working…' : mode === 'generate' ? 'Generate' : 'Import'}</Button></div>
  </form>);
}
