import { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { AtSign, Copy, Globe, Search, Timer } from 'lucide-react';
import { api } from '../../api/client';
import type { Filter, ItemSummary } from '../../api/types';
import { CategoryIcon } from '../../components/CategoryIcon';
import { cn } from '../../lib/utils';
import { Kbd } from '../../ui';
import { allCommands } from '../registry';
import { useT, translate, useI18n, type Key } from '../../i18n';
import { useApp } from '../../lib/store';
import { builtinCommands, type PaletteCommand } from './commands';
import { rank } from './fuzzy';
import { formatCombo, listShortcuts, matches, registerShortcut, useGlobalShortcuts } from './shortcuts';
import { Modal } from '../../ui';
import { itemShortcuts } from '../items/itemShortcuts';
import { copyPrimarySecret, copyTotpCode, copyUsername, openItemWebsite } from '../items/actions';

type Entry = { kind: 'cmd'; cmd: PaletteCommand } | { kind: 'item'; item: ItemSummary };
type ItemAction = { id: string; labelKey: Key; combo: string; aliases?: string[]; icon: typeof Copy; run: (item: ItemSummary) => void | Promise<void> };

/** Copy/open actions for the highlighted item, reusing the itemShortcuts actions (⌘⇧O stays an alias for open website). */
export const paletteItemActions: ItemAction[] = [
  { id: 'copy-password', labelKey: 'itemKeys.copyPassword', combo: 'mod+c', icon: Copy, run: (x) => copyPrimarySecret(x.vaultId, x.id) },
  { id: 'copy-username', labelKey: 'itemKeys.copyUsername', combo: 'mod+shift+c', icon: AtSign, run: (x) => copyUsername(x.vaultId, x.id) },
  { id: 'copy-totp', labelKey: 'itemKeys.copyTotp', combo: 'mod+alt+c', icon: Timer, run: (x) => copyTotpCode(x.vaultId, x.id) },
  { id: 'open-website', labelKey: 'itemKeys.openWebsite', combo: 'mod+o', aliases: ['mod+shift+o'], icon: Globe, run: (x) => openItemWebsite(x.vaultId, x.id) },
];

/** Empty-query results: most recently used first, then recently edited, capped at 8. */
export function recentItems(items: ItemSummary[], max = 8): ItemSummary[] {
  return [...items].sort((a, b) => (b.lastUsed ?? 0) - (a.lastUsed ?? 0) || b.updatedAt.localeCompare(a.updatedAt)).slice(0, max);
}

export function buildEntries(query: string, cmds: PaletteCommand[], items: ItemSummary[], label: (c: PaletteCommand) => string): Entry[] {
  const c = (query ? rank(query, cmds, (x) => [label(x), ...(x.keywords ?? [])]) : cmds).map((cmd) => ({ kind: 'cmd' as const, cmd }));
  const i = (query ? rank(query, items, (x) => [x.title, x.subtitle, x.url ?? '']) : recentItems(items)).map((item) => ({ kind: 'item' as const, item }));
  return query ? [...c, ...i] : [...i, ...c];
}

const Hint = ({ combo, label }: { combo: string; label: string }) => (
  <span className="flex items-center gap-1">{formatCombo(combo).map((k) => <Kbd key={k}>{k}</Kbd>)}<span>{label}</span></span>
);
const hasTextSelection = () => !!window.getSelection()?.toString();

export function CommandPalette({ open, onOpenChange, extraCommands = [] }: { open: boolean; onOpenChange: (o: boolean) => void; extraCommands?: PaletteCommand[] }) {
  const t = useT(); const lang = useI18n((s) => s.lang);
  const [q, setQ] = useState(''); const [items, setItems] = useState<ItemSummary[]>([]); const [sel, setSel] = useState(0);
  const [actionItem, setActionItem] = useState<ItemSummary | null>(null); const [actionSel, setActionSel] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  // palette builtins + every feature's registry commands (first id wins) + caller extras
  const cmds = useMemo(() => { const seen = new Set<string>(); return [...builtinCommands(), ...allCommands(), ...extraCommands].filter((c) => !seen.has(c.id) && !!seen.add(c.id)); }, [extraCommands, open]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (!open) { setQ(''); setSel(0); setActionItem(null); setActionSel(0); return; } let live = true;
    const filter: Filter = q ? { kind: 'all', query: q } : { kind: 'all' };
    const h = setTimeout(() => api.listItems({ filter }).then((r) => live && setItems(r)).catch(() => live && setItems([])), 80);
    return () => { live = false; clearTimeout(h); }; }, [q, open]);
  const entries = useMemo(() => buildEntries(q, cmds, items, (c) => (c.labelKey ? translate(lang, c.labelKey) : c.label)), [q, cmds, items, lang]);
  useEffect(() => setSel(0), [q]);
  useEffect(() => { listRef.current?.querySelector(`[data-idx="${sel}"]`)?.scrollIntoView({ block: 'nearest' }); }, [sel]);
  if (!open) return null;
  const run = (e: Entry) => { onOpenChange(false);
    if (e.kind === 'cmd') void e.cmd.run(); else { const a = useApp.getState(); a.setRoute({ name: 'items', filter: { kind: 'all' } }); a.selectItem(e.item.id); } };
  const runAction = (item: ItemSummary, action: ItemAction) => { onOpenChange(false); void action.run(item); };
  const openActions = (item: ItemSummary) => { setActionItem(item); setActionSel(0); };
  // ⌘C stays a native copy while the query has selected text.
  const shortcutAction = (e: React.KeyboardEvent) =>
    paletteItemActions.find((a) => (matches(a.combo, e) || !!a.aliases?.some((c) => matches(c, e))) && !(a.id === 'copy-password' && hasTextSelection()));
  const onKey = (e: React.KeyboardEvent) => {
    if (actionItem) {
      if (e.key === 'ArrowDown') { e.preventDefault(); setActionSel((s) => Math.min(s + 1, paletteItemActions.length - 1)); }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setActionSel((s) => Math.max(s - 1, 0)); }
      else if (e.key === 'Enter') { e.preventDefault(); const a = paletteItemActions[actionSel]; if (a) runAction(actionItem, a); }
      else if (e.key === 'Escape' || e.key === 'ArrowLeft') { e.preventDefault(); e.stopPropagation(); setActionItem(null); }
      else { const a = shortcutAction(e); if (a) { e.preventDefault(); e.stopPropagation(); runAction(actionItem, a); } }
      return;
    }
    const entry = entries[sel];
    const shortcut = entry?.kind === 'item' ? shortcutAction(e) : undefined;
    if (e.key === 'ArrowDown') { e.preventDefault(); setSel((s) => Math.min(s + 1, entries.length - 1)); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (entry?.kind === 'item' && (e.key === 'ArrowRight' || matches('mod+k', e))) { e.preventDefault(); e.stopPropagation(); openActions(entry.item); }
    else if (shortcut && entry?.kind === 'item') { e.preventDefault(); e.stopPropagation(); runAction(entry.item, shortcut); }
    else if (e.key === 'Enter' && entry) { e.preventDefault(); run(entry); }
    else if (e.key === 'Escape') { e.preventDefault(); onOpenChange(false); } };
  const firstCmd = entries.findIndex((e) => e.kind === 'cmd');
  const firstItem = entries.findIndex((e) => e.kind === 'item');
  const selected = entries[sel];
  return createPortal(<div className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-[12vh]" onMouseDown={(e) => e.target === e.currentTarget && onOpenChange(false)}>
    <div role="dialog" aria-modal="true" aria-label={t('palette.label')} className="w-full max-w-xl overflow-hidden rounded-xl border border-border bg-panel shadow-2xl motion-safe:animate-[fadeIn_.12s_ease-out]">
      <div className="flex items-center gap-3 border-b border-border px-4"><Search className="h-4 w-4 text-muted" aria-hidden />
        <input autoFocus role="combobox" aria-expanded aria-controls="palette-list" aria-activedescendant={actionItem ? `pal-${actionSel}` : selected ? `pal-${sel}` : undefined} aria-label={t('palette.placeholder')}
          className="h-12 flex-1 bg-transparent text-sm outline-none placeholder:text-muted" placeholder={t('palette.placeholder')} value={q} onChange={(e) => { setQ(e.target.value); setActionItem(null); }} onKeyDown={onKey} />
        <Kbd>Esc</Kbd></div>
      <ul id="palette-list" role="listbox" ref={listRef} aria-label={actionItem ? t('palette.actionsFor', { title: actionItem.title }) : undefined} className="max-h-96 overflow-auto p-2">
        {actionItem ? <>
          <li><div className="px-3 pb-1 pt-2 text-xs font-medium uppercase text-muted">{t('palette.actionsFor', { title: actionItem.title })}</div></li>
          {paletteItemActions.map((a, i) => <li key={a.id}>
            <div id={`pal-${i}`} data-idx={i} role="option" aria-selected={actionSel === i} onMouseMove={() => setActionSel(i)} onClick={() => runAction(actionItem, a)}
              className={cn('flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm', actionSel === i && 'bg-accent/20')}>
              <a.icon className="h-4 w-4 text-muted" aria-hidden /><span className="flex-1">{t(a.labelKey)}</span>
              <span className="flex gap-1">{formatCombo(a.combo).map((k) => <Kbd key={k}>{k}</Kbd>)}</span>
            </div></li>)}
        </> : <>
          {entries.length === 0 && <li className="px-3 py-8 text-center text-sm text-muted">{t('palette.empty', { q })}</li>}
          {entries.map((e, i) => <li key={e.kind === 'cmd' ? 'c' + e.cmd.id : 'i' + e.item.id}>
            {i === firstCmd && e.kind === 'cmd' && <div className="px-3 pb-1 pt-2 text-xs font-medium uppercase text-muted">{t('palette.commands')}</div>}
            {q && i === firstItem && e.kind === 'item' && <div className="px-3 pb-1 pt-2 text-xs font-medium uppercase text-muted">{t('palette.items')}</div>}
            <div id={`pal-${i}`} data-idx={i} role="option" aria-selected={sel === i} onMouseMove={() => setSel(i)} onClick={() => run(e)}
              className={cn('flex cursor-pointer items-center gap-3 rounded-md px-3 py-2 text-sm', sel === i && 'bg-accent/20')}>
              {e.kind === 'cmd' ? <>{e.cmd.icon ? <e.cmd.icon className="h-4 w-4 text-muted" aria-hidden /> : <span className="w-4" />}<span className="flex-1">{e.cmd.labelKey ? translate(lang, e.cmd.labelKey) : e.cmd.label}</span>
                {e.cmd.shortcut && <span className="flex gap-1">{formatCombo(e.cmd.shortcut).map((k) => <Kbd key={k}>{k}</Kbd>)}</span>}</>
                : <><CategoryIcon category={e.item.category} size={18} /><span className="flex-1 truncate">{e.item.title}<span className="ml-2 text-muted">{e.item.subtitle}</span></span></>}
            </div></li>)}
        </>}
      </ul>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-border px-4 py-2 text-xs text-muted">
        {actionItem ? <><Hint combo="enter" label={t('palette.run')} /><span className="flex items-center gap-1"><Kbd>Esc</Kbd><span>{t('palette.back')}</span></span></>
          : selected?.kind === 'item' ? <>{paletteItemActions.map((a) => <Hint key={a.id} combo={a.combo} label={t(a.labelKey)} />)}<Hint combo="enter" label={t('palette.openItem')} /><span className="flex items-center gap-1"><Kbd>→</Kbd><span>{t('palette.actions')}</span></span></>
          : <Hint combo={selected?.kind === 'cmd' && selected.cmd.shortcut ? selected.cmd.shortcut : 'enter'} label={t('palette.run')} />}
      </div></div></div>, document.body);
}

export function ShortcutsHelp({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const t = useT(); const lang = useI18n((s) => s.lang);
  return <Modal open={open} onOpenChange={onOpenChange} title={t('shortcuts.title')}>
    <ul className="space-y-2">{listShortcuts().map((s) => <li key={s.id} className="flex items-center justify-between text-sm"><span>{translate(lang, s.labelKey)}</span>
      <span className="flex gap-1">{formatCombo(s.combo).map((k) => <Kbd key={k}>{k}</Kbd>)}</span></li>)}</ul></Modal>;
}

/** Mount once in the shell (e.g. App.tsx). Registers global shortcuts and renders palette + help. */
export function PaletteHost() {
  const [open, setOpen] = useState(false); const modal = useApp((s) => s.modal); const closeModal = useApp((s) => s.closeModal);
  useGlobalShortcuts();
  useEffect(() => { const app = () => useApp.getState(); const offs = [
    registerShortcut({ id: 'palette', combo: 'mod+k', labelKey: 'shortcuts.palette', run: () => setOpen((o) => !o), allowInInput: true }),
    registerShortcut({ id: 'help', combo: '?', labelKey: 'shortcuts.help', run: () => app().openModal('shortcuts-help') }),
    registerShortcut({ id: 'new-item', combo: 'mod+n', labelKey: 'shortcuts.newItem', run: () => app().openModal('new-item') }),
    registerShortcut({ id: 'generate-password', combo: 'mod+shift+g', labelKey: 'shortcuts.generatePassword', run: () => app().openModal('password-generator') }),
    ...itemShortcuts.map(registerShortcut),
    registerShortcut({ id: 'lock', combo: 'mod+shift+l', labelKey: 'shortcuts.lock', run: () => { void api.lock().catch(() => {}); app().lock(); }, allowInInput: true }),
  ]; return () => offs.forEach((f) => f()); }, []);
  useEffect(() => { if (modal?.id === 'palette') { setOpen(true); closeModal(); } }, [modal, closeModal]);
  return <><CommandPalette open={open} onOpenChange={setOpen} /><ShortcutsHelp open={modal?.id === 'shortcuts-help'} onOpenChange={(o) => !o && closeModal()} /></>;
}
