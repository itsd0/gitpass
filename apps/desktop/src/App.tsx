import { useEffect, useState } from 'react';
import { AppShell } from './layout/AppShell';
import { Unlock } from './layout/Unlock';
import { useApp } from './lib/store';
import { Toaster, TooltipProvider } from './ui';
import { api } from './api/client';
import { QuickAccessPage, isQuickAccessWindow } from './features/quick-access';
import { useAgentEvents } from './lib/agentEvents';

/** Dedicated quick-access window (`#/quick-access` or `?window=quick-access`): no shell, no unlock form;
 * the Rust session is shared with the main window, so it only works while that is unlocked. */
function QuickAccessWindow() {
  const [state, setState] = useState<'loading' | 'locked' | 'ok'>('loading');
  useEffect(() => { api.status().then((s) => setState(s.unlocked ? 'ok' : 'locked'), () => setState('locked')); }, []);
  if (state === 'ok') return <QuickAccessPage />;
  return <div className="flex h-screen items-center justify-center bg-bg text-sm text-muted">{state === 'locked' ? 'Unlock gitpass in the main window first.' : '…'}</div>;
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
