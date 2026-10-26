import { useEffect, useRef } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { ItemsPage } from '../features/items';
import { featureForRoute, modalFor } from '../features/registry';
import { useApp } from '../lib/store';
import { Sidebar } from './Sidebar';
import { PaletteHost } from '../features/palette';
import { useSettings } from '../features/settings/useSettings';
import { Toolbar } from './Toolbar';

function RouteView() {
  const route = useApp((s) => s.route);
  if (route.name === 'items') return <ItemsPage />;
  const F = featureForRoute(route.name)?.Page;
  return F ? <F /> : <div className="flex h-full items-center justify-center text-muted">“{route.name}” arrives with its feature stream.</div>;
}

export function AppShell() {
  const { sidebarCollapsed, modal } = useApp();
  const search = useRef<HTMLInputElement>(null);
  const M = modal ? modalFor(modal.id) : undefined;
  // Rust-persisted settings drive theme + language once unlocked.
  useEffect(() => { void useSettings.getState().load(); }, []);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') { e.preventDefault(); search.current?.focus(); search.current?.select(); }
      if ((e.metaKey || e.ctrlKey) && e.key === '\\') { e.preventDefault(); useApp.getState().toggleSidebar(); }
    };
    window.addEventListener('keydown', onKey); return () => window.removeEventListener('keydown', onKey);
  }, []);
  return (
    <div className="h-full">
      <PanelGroup direction="horizontal">
        {!sidebarCollapsed && <>
          <Panel id="sidebar" order={1} defaultSize={18} minSize={13} maxSize={28}><Sidebar /></Panel>
          <PanelResizeHandle className="w-px bg-border hover:bg-accent" />
        </>}
        <Panel id="main" order={2}>
          <div className="flex h-full flex-col"><Toolbar ref={search} /><div className="min-h-0 flex-1"><RouteView /></div></div>
        </Panel>
      </PanelGroup>
      {M && <M />}
      <PaletteHost />
    </div>
  );
}
