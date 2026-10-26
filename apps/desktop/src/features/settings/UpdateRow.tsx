import { useState } from 'react';
import { Button } from '../../ui';
import { useT } from '../../i18n';
import { checkForUpdate, type UpdateCheck } from '../../lib/updater';

type State = { s: 'idle' } | { s: 'checking' } | { s: 'done'; r: UpdateCheck } | { s: 'installing'; pct: number | null } | { s: 'error'; msg: string };

export function UpdateRow({ check = checkForUpdate }: { check?: () => Promise<UpdateCheck> }) {
  const t = useT(); const [st, setSt] = useState<State>({ s: 'idle' });
  const run = async () => { setSt({ s: 'checking' }); try { setSt({ s: 'done', r: await check() }); } catch (e) { setSt({ s: 'error', msg: String((e as Error)?.message ?? e) }); } };
  const avail = st.s === 'done' && st.r.kind === 'available' ? st.r : null;
  const msg = st.s === 'checking' ? t('update.checking')
    : st.s === 'installing' ? t('update.installing', { pct: st.pct == null ? '…' : `${st.pct}%` })
    : st.s === 'error' ? t('update.error', { msg: st.msg })
    : st.s === 'done' ? (st.r.kind === 'none' ? t('update.none') : st.r.kind === 'unsupported' ? t('update.unsupported') : t('update.available', { version: st.r.version }))
    : '';
  return <div className="flex items-start justify-between gap-6 border-b border-border py-4 last:border-0">
    <div><div className="text-sm font-medium">{t('update.title')}</div>{msg && <div role="status" className="mt-0.5 text-xs text-muted">{msg}</div>}</div>
    <div className="flex shrink-0 gap-2">
      {avail && <Button variant="default" size="sm" onClick={async () => { setSt({ s: 'installing', pct: null }); try { await avail.install((pct) => setSt({ s: 'installing', pct })); } catch (e) { setSt({ s: 'error', msg: String((e as Error)?.message ?? e) }); } }}>{t('update.install')}</Button>}
      <Button variant="outline" size="sm" disabled={st.s === 'checking' || st.s === 'installing'} onClick={run}>{t('update.check')}</Button>
    </div></div>;
}
