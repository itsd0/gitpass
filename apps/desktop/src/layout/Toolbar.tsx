import { forwardRef } from 'react';
import { ArrowLeft, ArrowRight, KeyRound, PanelLeftOpen, Plus, Search } from 'lucide-react';
import { useApp } from '../lib/store';
import { useItemsUI } from '../features/items';
import { Button, Kbd, Tooltip } from '../ui';
import { useT } from '../i18n';

export const Toolbar = forwardRef<HTMLInputElement>(function Toolbar(_, ref) {
  const { sidebarCollapsed, toggleSidebar, openModal, route, setRoute } = useApp();
  const { query, setQuery } = useItemsUI();
  const t = useT();
  const isMac = typeof navigator !== 'undefined' && /Mac/.test(navigator.platform);
  return (
    <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border bg-panel px-3">
      {sidebarCollapsed && <Button variant="ghost" size="icon" aria-label="Show sidebar" onClick={toggleSidebar}><PanelLeftOpen size={16} /></Button>}
      <Button variant="ghost" size="iconSm" aria-label="Back" onClick={() => history.back()}><ArrowLeft size={15} /></Button>
      <Button variant="ghost" size="iconSm" aria-label="Forward" onClick={() => history.forward()}><ArrowRight size={15} /></Button>
      <label className="flex h-8 flex-1 items-center gap-2 rounded-lg border border-border bg-bg px-2.5 focus-within:border-accent focus-within:ring-2 focus-within:ring-accent/40">
        <Search size={15} className="text-muted" />
        <input ref={ref} aria-label="Search" value={query} placeholder="Search in gitpass"
          onChange={(e) => { setQuery(e.target.value); if (route.name !== 'items') setRoute({ name: 'items', filter: { kind: 'all', sort: 'updated' } }); }}
          onKeyDown={(e) => { if (e.key === 'Escape') { setQuery(''); (e.target as HTMLInputElement).blur(); } if (e.key === 'ArrowDown') (document.querySelector('[data-item-list]') as HTMLElement | null)?.focus(); }}
          className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted" />
        <Kbd>{isMac ? '⌘' : 'Ctrl'}</Kbd><Kbd>F</Kbd>
      </label>
      <Tooltip content={t('generator.title')}>
        <Button variant="ghost" size="iconSm" aria-label={t('generator.title')} onClick={() => openModal('password-generator')}>
          <KeyRound size={15} />
        </Button>
      </Tooltip>
      <Button variant="link" size="sm" className="px-2" onClick={() => setRoute({ name: 'settings', tab: 'help' })}>Help</Button>
      <Button onClick={() => openModal('new-item')}><Plus size={15} />{t('nav.newItem')}</Button>
    </div>
  );
});
