import { useEffect, useRef, useState } from 'react';
import { KeyRound, Search, Timer } from 'lucide-react';
import { api } from '../../api/client';
import type { Field, ItemDetail, ItemSummary } from '../../api/types';
import { cn } from '../../lib/utils';
import { Avatar, Kbd, toast } from '../../ui';
import { useT } from '../../i18n';
import { useApp } from '../../lib/store';
import { rank } from '../palette/fuzzy';

/** Pick the field ids used for copy actions from an item detail (secrets never leave Rust). */
export function pickFields(d: ItemDetail): { username?: Field; password?: Field; totp?: Field } {
  const all = d.sections.flatMap((s) => s.fields);
  return { username: all.find((f) => /user|email|login/i.test(f.label) && f.kind !== 'concealed') ?? all.find((f) => f.kind === 'email'),
    password: all.find((f) => f.kind === 'concealed' && /pass/i.test(f.label)) ?? all.find((f) => f.kind === 'concealed'), totp: all.find((f) => f.kind === 'totp') };
}

export function QuickAccess({ onDone }: { onDone?: () => void }) {
  const t = useT(); const [q, setQ] = useState(''); const [items, setItems] = useState<ItemSummary[]>([]); const [sel, setSel] = useState(0);
  const [detail, setDetail] = useState<ItemDetail | null>(null); const [totp, setTotp] = useState<{ code: string; remaining: number } | null>(null); const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => { let live = true; const h = setTimeout(() => api.listItems({ filter: { kind: 'all', query: q || undefined } })
    .then((r) => live && setItems(q ? rank(q, r, (x) => [x.title, x.subtitle, x.url ?? '']) : r)).catch(() => live && setItems([])), 60); return () => { live = false; clearTimeout(h); }; }, [q]);
  useEffect(() => setSel(0), [q]);
  const cur = items[sel];
  useEffect(() => { setDetail(null); setTotp(null); if (!cur) return; let live = true;
    api.getItem({ vaultId: cur.vaultId, itemId: cur.id }).then((d) => live && setDetail(d)).catch(() => {}); return () => { live = false; }; }, [cur?.id]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (!detail?.hasTotp) return; let live = true; const tick = () => api.totpNow({ vaultId: detail.vaultId, itemId: detail.id }).then((r) => live && setTotp(r)).catch(() => {});
    tick(); const iv = setInterval(tick, 1000); return () => { live = false; clearInterval(iv); }; }, [detail]);
  const copy = async (which: 'password' | 'username') => {
    if (!cur) return; const d = detail ?? await api.getItem({ vaultId: cur.vaultId, itemId: cur.id }); const f = pickFields(d)[which]; if (!f) return;
    try { await api.copyField({ vaultId: cur.vaultId, itemId: cur.id, fieldId: f.id }); toast(t(which === 'password' ? 'qa.passwordCopied' : 'qa.usernameCopied')); onDone?.(); }
    catch (e) { toast(t('common.error', { msg: String((e as Error)?.message ?? e) }), 'danger'); } };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(s + 1, items.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (e.key === 'Enter') { e.preventDefault(); void copy(e.metaKey || e.ctrlKey ? 'username' : 'password'); }
    else if (e.key === 'Escape') { e.preventDefault(); if (q) setQ(''); else onDone?.(); } };
  return <div role="dialog" aria-label={t('qa.label')} className="flex h-full w-full flex-col overflow-hidden rounded-xl border border-border bg-panel text-fg">
    <div className="flex items-center gap-3 border-b border-border px-4"><Search className="h-5 w-5 text-muted" aria-hidden />
      <input ref={inputRef} autoFocus role="combobox" aria-expanded aria-controls="qa-list" aria-activedescendant={cur ? `qa-${sel}` : undefined} aria-label={t('qa.placeholder')}
        className="h-14 flex-1 bg-transparent text-base outline-none placeholder:text-muted" placeholder={t('qa.placeholder')} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} /></div>
    <div className="flex min-h-0 flex-1">
      <ul id="qa-list" role="listbox" className="w-1/2 overflow-auto border-r border-border p-2">
        {items.length === 0 && <li className="px-3 py-8 text-center text-sm text-muted">{q ? t('qa.noResults') : t('qa.hint')}</li>}
        {items.map((it, i) => <li key={it.id} id={`qa-${i}`} role="option" aria-selected={sel === i} onMouseMove={() => setSel(i)} onClick={() => { setSel(i); inputRef.current?.focus(); }} onDoubleClick={() => copy('password')}
          className={cn('flex cursor-pointer items-center gap-3 rounded-md px-3 py-2', sel === i && 'bg-accent/20')}>
          <Avatar name={it.title} size={28} /><div className="min-w-0"><div className="truncate text-sm font-medium">{it.title}</div><div className="truncate text-xs text-muted">{it.subtitle}</div></div></li>)}
      </ul>
      <div className="flex w-1/2 flex-col p-4">{cur ? <>
        <div className="flex items-center gap-3"><Avatar name={cur.title} size={40} /><div className="min-w-0"><div className="truncate font-semibold">{cur.title}</div><div className="truncate text-sm text-muted">{cur.subtitle}</div></div></div>
        {totp && <div className="mt-4 rounded-lg border border-border bg-card p-3" aria-live="polite"><div className="flex items-center gap-1 text-xs text-muted"><Timer className="h-3 w-3" aria-hidden />{t('qa.totp')}</div>
          <div className="mt-1 flex items-baseline justify-between"><span className="font-mono text-2xl tracking-widest">{totp.code.replace(/(\d{3})(\d{3})/, '$1 $2')}</span><span className="text-xs text-muted">{totp.remaining}s</span></div></div>}
        <div className="mt-auto space-y-1.5 text-xs text-muted">
          <div className="flex items-center justify-between"><span className="flex items-center gap-1"><KeyRound className="h-3 w-3" aria-hidden />{t('qa.copyPassword')}</span><Kbd>↵</Kbd></div>
          <div className="flex items-center justify-between"><span>{t('qa.copyUsername')}</span><span className="flex gap-1"><Kbd>⌘/Ctrl</Kbd><Kbd>↵</Kbd></span></div></div></>
        : <div className="m-auto text-sm text-muted">{t('qa.hint')}</div>}</div></div></div>;
}
/** Standalone page for the quick-access window (route `#/quick-access` or `?window=quick-access`). */
export function QuickAccessPage() { return <div className="h-screen w-screen bg-bg p-0"><QuickAccess onDone={() => { const w = (window as any).__TAURI_INTERNALS__ ? import('@tauri-apps/api/window').then((m) => m.getCurrentWindow().hide()) : null; void w; }} /></div>; }

/** In-app route (browser/mock, or before the separate window is used): framed like the popup window. */
export function QuickAccessRoute() {
  return <div className="flex h-full items-start justify-center bg-bg pt-16"><div className="h-[380px] w-[560px] overflow-hidden rounded-xl border border-border bg-panel shadow-2xl">
    <QuickAccess onDone={() => useApp.getState().setRoute({ name: 'items', filter: { kind: 'all' } })} /></div></div>;
}
