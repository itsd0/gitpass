import { useEffect, useState } from 'react';
import { AppShell } from './layout/AppShell';
import { Unlock } from './layout/Unlock';
import { useApp } from './lib/store';
import { Toaster, TooltipProvider } from './ui';
import { api } from './api/client';
import { QuickAccessPage, isQuickAccessWindow } from './features/quick-access';
import { QuickAccessUnlock } from './features/quick-access/QuickAccessUnlock';
import { useAgentEvents } from './lib/agentEvents';

/** Dedicated quick-access window (`#/quick-access` or `?window=quick-access`).
 * When locked, displays QuickAccessUnlock with Touch ID if enabled; never shows item data before unlock. */
function QuickAccessWindow() {
  const locked = useApp((s) => s.locked);
  const [init, setInit] = useState(false);

  useEffect(() => {
    api.status().then((s) => {
      if (s.unlocked) {
        useApp.getState().setAccount(s.account);
        useApp.getState().unlock();
      } else {
        useApp.getState().lock();
      }
      setInit(true);
    }, () => {
      useApp.getState().lock();
      setInit(true);
    });
  }, []);

  if (!init) return <div className="flex h-screen items-center justify-center bg-bg text-sm text-muted">…</div>;
  if (locked) return <div className="h-screen w-screen bg-bg p-0"><QuickAccessUnlock /></div>;
  return <QuickAccessPage />;
}

export default function App() {
  const locked = useApp((s) => s.locked);
  const theme = useApp((s) => s.theme);
  useAgentEvents();
  useEffect(() => { document.documentElement.classList.toggle('dark', theme === 'dark'); }, [theme]);
  return (
    <TooltipProvider>
      {isQuickAccessWindow() ? <QuickAccessWindow /> : locked ? <Unlock /> : <AppShell />}
      <Toaster />
    </TooltipProvider>
  );
}
