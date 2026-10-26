import { useEffect, useState } from 'react';
import { CheckCircle2, Circle } from 'lucide-react';
import { EmergencyKitFlow } from './EmergencyKit';
import type { SyncStatus } from '../../api/types';
import { api } from '../../api/client';
import { Button } from '../../ui';
import { useApp } from '../../lib/store';
import { STEPS, stepsLeft } from './steps';
import { useT } from '../../i18n';
import { useOnboarding } from './useOnboarding';
/** Kept for compatibility: the printable kit (see EmergencyKit.tsx). */
export function EmergencyKitView({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  return <EmergencyKitFlow onClose={onClose} onPrinted={onSaved} />;
}
export function Checklist() {
  const { onboarding, patch } = useOnboarding(); const { openModal, setRoute } = useApp(); const t = useT();
  const [checked, setChecked] = useState<string | null>(null);
  const [kit, setKit] = useState(false); const [sync, setSync] = useState<SyncStatus | null>(null); const [showRepo, setShowRepo] = useState(false);
  useEffect(() => { api.syncStatus().then(setSync).catch(() => {}); }, []);
  const recheck = () => api.syncStatus().then(st => { setSync(st); if (st.remote) { patch({ repoConnected: true }); setChecked(null); } else setChecked(t('onb.repo.stillNot')); }).catch(e => setChecked(String((e as Error)?.message ?? e)));
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
        {!done && <Button size="sm" variant="outline" onClick={actions[s.key]}>{s.key === 'repoConnected' ? t('onb.repo.how') : 'Start'}</Button>}</div>
      {s.key === 'repoConnected' && showRepo && (<div className="mt-3 space-y-2 rounded bg-bg p-3 text-xs text-fg">
        <div>{t('onb.repo.status')}: {sync ? (sync.remote ? <><code>{sync.remote}</code> · {t('sync.ahead', { n: sync.ahead })} / {t('sync.behind', { n: sync.behind })}</> : t('onb.repo.noRemote')) : '…'}</div>
        <ol className="list-decimal space-y-1 pl-4"><li>{t('onb.repo.step1')}</li><li>{t('onb.repo.step2')}</li><li>{t('onb.repo.step3')}</li></ol>
        <div className="text-muted">{t('onb.repo.cli')}</div>
        <div className="flex gap-2"><Button size="sm" variant="default" onClick={() => setRoute({ name: 'settings', tab: 'sync' })}>{t('onb.repo.open')}</Button>
          <Button size="sm" variant="outline" onClick={recheck}>{t('onb.repo.recheck')}</Button></div>
        {checked && <div role="status" className="text-muted">{checked}</div>}</div>)}
    </div>); })}
    {kit && <EmergencyKitView onClose={() => setKit(false)} onSaved={() => patch({ emergencyKitSaved: true })} />}
  </div>);
}
