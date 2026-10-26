import { useEffect, useState } from 'react';
import { CheckCircle2, Circle, Printer, X } from 'lucide-react';
import type { SyncStatus } from '../../api/types';
import { api } from '../../api/client';
import { Button } from '../../ui';
import { useApp } from '../../lib/store';
import { STEPS, stepsLeft } from './steps';
import { useOnboarding } from './useOnboarding';
export function EmergencyKitView({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [kit, setKit] = useState<string | null>(null); const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.emergencyKit().then(setKit).catch(e => setErr(String(e))); }, []);
  const isHtml = kit !== null && /^\s*</.test(kit);
  return (<div className="fixed inset-0 z-50 overflow-auto bg-white text-black" role="dialog" aria-label="Emergency Kit">
    <div className="no-print flex justify-end gap-2 border-b p-3"><Button variant="default" onClick={() => { window.print(); onSaved(); }}><Printer size={14} /> Print / Save as PDF</Button><Button variant="ghost" className="text-black" onClick={onClose}><X size={14} /> Close</Button></div>
    <div className="mx-auto max-w-2xl p-10"><h1 className="mb-1 text-2xl font-bold">gitpass Emergency Kit</h1><p className="mb-6 text-sm text-muted">Keep this somewhere safe and offline. You need it to recover your vault.</p>
      {err && <p className="text-red-600">{err}</p>}
      {kit !== null && (isHtml ? <iframe title="kit" sandbox="" srcDoc={kit} className="h-[600px] w-full border" /> : <pre className="whitespace-pre-wrap rounded border p-4 font-mono text-sm">{kit}</pre>)}</div></div>);
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
