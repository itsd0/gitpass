import { useEffect, useMemo, useState } from 'react';
import { ArrowLeft, Eye, EyeOff, Paperclip } from 'lucide-react';
import type { FieldKind, ItemCategory, VaultInfo } from '../../api/types';
import { api } from '../../api/client';
import { Button, Input } from '../../ui';
import { useApp } from '../../lib/store';
import { buildSections, categoryById } from './categories';
import { brandLabel, detectBrand, formatCardNumber, formatExpiry, luhnValid } from './card';
import { CategoryIcon } from './CategoryIcon';
import { PasswordGenerator, StrengthMeter } from './PasswordGenerator';
const MONTHS = Array.from({ length: 12 }, (_, i) => String(i + 1).padStart(2, '0'));
const YEARS = Array.from({ length: 21 }, (_, i) => String(new Date().getFullYear() - 5 + i));
/** Accepts a raw base32 secret or an otpauth:// URI (e.g. pasted from a QR scanner). Returns normalised value or null. */
export function normaliseTotp(input: string): string | null {
  const s = input.trim(); if (!s) return null;
  if (s.startsWith('otpauth://')) { try { const u = new URL(s); return u.searchParams.get('secret') ? s : null; } catch { return null; } }
  const b = s.replace(/\s+/g, '').toUpperCase(); return /^[A-Z2-7]+=*$/.test(b) && b.length >= 16 ? b : null;
}
export function CreateForm({ category, initial, vaultId: initialVault, onBack, onDone }: { category: ItemCategory; initial?: { title?: string; url?: string }; vaultId?: string; onBack: () => void; onDone: () => void }) {
  const cat = categoryById(category);
  const [vaults, setVaults] = useState<VaultInfo[]>([]);
  const [vaultId, setVaultId] = useState(initialVault ?? '');
  const [title, setTitle] = useState(initial?.title ?? '');
  const [values, setValues] = useState<Record<string, string>>(() => (initial?.url ? { 'main.website': initial.url } : {}) as Record<string, string>);
  const [notes, setNotes] = useState(''); const [docPath, setDocPath] = useState('');
  const [shown, setShown] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false); const [err, setErr] = useState<string | null>(null);
  const { setRoute, selectItem } = useApp();
  useEffect(() => { api.listVaults().then(v => { setVaults(v); setVaultId(id => id || v[0]?.id || ''); }).catch(e => setErr(String(e))); }, []);
  const set = (k: string, v: string) => setValues(s => ({ ...s, [k]: v }));
  const cardNo = values['main.number'] ?? ''; const brand = useMemo(() => detectBrand(cardNo), [cardNo]);
  const totpKey = Object.keys(values).find(k => k.endsWith('.otp'));
  const totpInvalid = totpKey && values[totpKey] && !normaliseTotp(values[totpKey]);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault(); if (!title.trim() || !vaultId) { setErr('Title and vault are required.'); return; }
    if (totpInvalid) { setErr('One-time password must be a base32 secret or otpauth:// URI.'); return; }
    if (category === 'document' && !docPath.trim()) { setErr('Choose a file to attach.'); return; }
    setBusy(true); setErr(null);
    try {
      const v = { ...values }; if (totpKey && v[totpKey]) v[totpKey] = normaliseTotp(v[totpKey])!;
      if (category === 'credit_card' && !v['main.type'] && brand !== 'unknown') v['main.type'] = brandLabel[brand];
      const item = await api.createItem({ input: { vaultId, category, title: title.trim(), sections: buildSections(category, v), notes: notes.trim() || null, url: v['main.website'] || v['main.hostname'] || null } });
      if (category === 'document') await api.addAttachment({ vaultId, itemId: item.id, path: docPath.trim() });
      setRoute({ name: 'items', filter: { kind: 'vault', vaultId } }); selectItem(item.id); onDone();
    } catch (e) { setErr(String(e)); } finally { setBusy(false); }
  };
  const renderField = (sid: string, fid: string, label: string, kind: FieldKind) => {
    const key = `${sid}.${fid}`; const val = values[key] ?? '';
    if (category === 'credit_card' && fid === 'number') return (<div>
      <div className="relative"><Input aria-label={label} inputMode="numeric" autoComplete="off" value={val} onChange={e => set(key, formatCardNumber(e.target.value))} placeholder="1234 5678 9012 3456" />
        {brand !== 'unknown' && <span data-testid="card-brand" className="absolute right-2 top-1.5 rounded bg-hover px-1.5 py-0.5 text-xs">{brandLabel[brand]}</span>}</div>
      {val.replace(/\D/g, '').length >= 12 && !luhnValid(val) && <p className="mt-1 text-xs text-warn">This number doesn't pass the checksum.</p>}</div>);
    if (kind === 'month_year') { const [m = '', y = ''] = val.split('/'); return (<div className="flex gap-2">
      <select aria-label={`${label} month`} className="h-9 rounded-md border border-border bg-panel px-2 text-sm" value={m} onChange={e => set(key, formatExpiry(e.target.value, y || YEARS[5]))}><option value="">MM</option>{MONTHS.map(x => <option key={x}>{x}</option>)}</select>
      <select aria-label={`${label} year`} className="h-9 rounded-md border border-border bg-panel px-2 text-sm" value={y} onChange={e => set(key, formatExpiry(m || '01', e.target.value))}><option value="">YYYY</option>{YEARS.map(x => <option key={x}>{x}</option>)}</select></div>); }
    if (kind === 'multiline') return <textarea aria-label={label} rows={fid === 'note' ? 8 : 3} className="w-full rounded-md border border-border bg-panel p-2 text-sm outline-none focus:border-accent" value={val} onChange={e => set(key, e.target.value)} />;
    if (kind === 'concealed') { const isPw = fid === 'password'; return (<div>
      <div className="flex items-center gap-1"><Input aria-label={label} type={shown[key] ? 'text' : 'password'} autoComplete="new-password" className="font-mono" value={val} onChange={e => set(key, e.target.value)} />
        <Button type="button" size="icon" variant="ghost" aria-label={shown[key] ? 'Hide' : 'Show'} onClick={() => setShown(s => ({ ...s, [key]: !s[key] }))}>{shown[key] ? <EyeOff size={14} /> : <Eye size={14} />}</Button>
        {isPw && <PasswordGenerator onUse={pw => { set(key, pw); setShown(s => ({ ...s, [key]: true })); }} />}</div>
      {isPw && <StrengthMeter password={val} />}</div>); }
    if (kind === 'totp') return (<div><Input aria-label={label} placeholder="Paste secret or otpauth:// URI from QR code" value={val} onChange={e => set(key, e.target.value)} onPaste={e => { const t = e.clipboardData.getData('text'); if (t) { e.preventDefault(); set(key, t.trim()); } }} />
      {totpInvalid && <p className="mt-1 text-xs text-warn">Not a valid base32 secret or otpauth:// URI.</p>}</div>);
    const type = kind === 'email' ? 'email' : kind === 'phone' ? 'tel' : kind === 'date' ? 'date' : kind === 'url' ? 'url' : 'text';
    return <Input aria-label={label} type={type} value={val} onChange={e => set(key, e.target.value)} />;
  };
  return (<form onSubmit={submit} className="space-y-4" aria-label={`New ${cat.label}`}>
    <div className="flex items-center gap-3">
      <Button type="button" size="icon" variant="ghost" aria-label="Back" onClick={onBack}><ArrowLeft size={16} /></Button>
      <CategoryIcon category={category} size={40} />
      <Input aria-label="Title" autoFocus className="h-11 border-transparent bg-transparent text-xl font-semibold" placeholder={cat.label} value={title} onChange={e => setTitle(e.target.value)} /></div>
    <label className="block text-xs text-muted">Vault
      <select aria-label="Vault" className="mt-1 h-9 w-full rounded-md border border-border bg-panel px-2 text-sm text-fg" value={vaultId} onChange={e => setVaultId(e.target.value)}>{vaults.map(v => <option key={v.id} value={v.id}>{v.name}</option>)}</select></label>
    {cat.sections.map(s => (<fieldset key={s.id} className="rounded-lg border border-border p-3">
      {s.title && <legend className="px-1 text-xs font-semibold uppercase tracking-wide text-muted">{s.title}</legend>}
      <div className="space-y-3">{s.fields.map(fl => (<div key={fl.id}><div className="mb-1 text-xs text-accent">{fl.label}</div>{renderField(s.id, fl.id, fl.label, fl.kind)}</div>))}</div></fieldset>))}
    {category === 'document' && (<div className="rounded-lg border border-dashed border-border p-4">
      <div className="mb-2 flex items-center gap-2 text-sm"><Paperclip size={14} /> File to attach (added after the item is created)</div>
      <Input aria-label="File path" placeholder="/path/to/file.pdf" value={docPath} onChange={e => setDocPath(e.target.value)} /></div>)}
    {category !== 'secure_note' && (<div><div className="mb-1 text-xs text-muted">notes</div>
      <textarea aria-label="Notes" rows={3} className="w-full rounded-md border border-border bg-panel p-2 text-sm outline-none focus:border-accent" value={notes} onChange={e => setNotes(e.target.value)} /></div>)}
    {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={onBack}>Cancel</Button><Button type="submit" variant="default" disabled={busy}>{busy ? 'Saving…' : 'Save'}</Button></div>
  </form>);
}
