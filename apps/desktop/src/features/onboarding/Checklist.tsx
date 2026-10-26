import { useEffect, useState } from 'react';
import { CheckCircle2, Circle } from 'lucide-react';
import { EmergencyKitFlow } from './EmergencyKit';
import type { SyncStatus } from '../../api/types';
import { api } from '../../api/client';
import { Button } from '../../ui';
import { useApp } from '../../lib/store';
import { STEPS, stepsLeft } from './steps';
import { useOnboarding } from './useOnboarding';
/** Kept for compatibility: the printable kit (see EmergencyKit.tsx). */
export function EmergencyKitView({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  return <EmergencyKitFlow onClose={onClose} onPrinted={onSaved} />;
}
export function Checklist() {
  const { onboarding, patch } = useOnboarding(); const { openModal } = useApp();
  const [kit, setKit] = useState(false); const [sync, setSync] = useState<SyncStatus | null>(null); const [showRepo, setShowRepo] = useState(false);
  useEffect(() => { api.syncStatus().then(setSync).catch(() => {}); }, []);
  if (!onboarding) return null;
  const actions: Record<string, () => void> = {
    extensionInstalled: () => patch({ extensionInstalled: true }),
    emergencyKitSaved: () => setKit(true),
    repoConnected: () => setShowRepo(s => !s),
    imported: () => openModal('new-item', { view: 'import' }),
  };
  return (<div aria-label="Onboarding checklist" className="space-y-2">
    <div className="text-xs text-muted">{stepsLeft(onboarding)} of {STEPS.length} steps left</div>
    {STEPS.map(s => { const done = onboarding[s.key]; return (<div key={s.key} className="rounded-lg border border-border p-3">
      <div className="flex items-center gap-3">{done ? <CheckCircle2 size={18} className="text-ok" /> : <Circle size={18} className="text-muted" />}
        <div className="flex-1"><div className={done ? 'text-sm text-muted line-through' : 'text-sm font-medium'}>{s.label}</div><div className="text-xs text-muted">{s.desc}</div></div>
        {!done && <Button size="sm" variant="outline" onClick={actions[s.key]}>{s.key === 'repoConnected' ? 'How?' : 'Start'}</Button>}</div>
      {s.key === 'repoConnected' && showRepo && (<div className="mt-3 space-y-2 rounded bg-bg p-3 text-xs text-fg">
        <div>Sync status: {sync ? (sync.remote ? <>remote <code>{sync.remote}</code>, last sync {sync.lastSync ?? 'never'}, {sync.ahead} ahead / {sync.behind} behind{sync.dirty ? ', uncommitted changes' : ''}</> : 'no remote configured') : 'unknown'}</div>
        <ol className="list-decimal space-y-1 pl-4"><li>Create a <b>private</b> GitHub repository (empty).</li><li>Run <code>gitpass remote add origin git@github.com:you/vault.git</code> in a terminal (or set it in Settings → Sync).</li><li>Run <code>gitpass sync</code>. Only encrypted data is pushed.</li></ol>
        <Button size="sm" variant="outline" onClick={() => api.syncStatus().then(st => { setSync(st); if (st.remote) patch({ repoConnected: true }); })}>Re-check</Button></div>)}
    </div>); })}
    {kit && <EmergencyKitView onClose={() => setKit(false)} onSaved={() => patch({ emergencyKitSaved: true })} />}
  </div>);
}
