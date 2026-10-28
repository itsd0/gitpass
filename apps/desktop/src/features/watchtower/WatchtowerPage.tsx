import { useCallback, useEffect, useState } from 'react';
import { AlertTriangle, Clock, Copy, KeyRound, ShieldAlert, ShieldCheck, Smartphone, RefreshCw, CalendarX, CalendarClock, Globe, ExternalLink, Pencil } from 'lucide-react';
import type { ItemSummary, WatchtowerReport } from '../../api/types';
import { api } from '../../api/client';
import { useT } from '../../i18n';
import { Button, Skeleton } from '../../ui';
import { useApp } from '../../lib/store';
import { useSettings } from '../settings/useSettings';
import { gaugeDash, issueIds, scoreBand, type IssueKey } from './score';
export function ScoreGauge({ score }: { score: number }) {
  const b = scoreBand(score); const d = gaugeDash(score);
  return (<div className="relative h-40 w-40" role="img" aria-label={`Security score ${b.score} of 100, ${b.label}`}>
    <svg viewBox="0 0 120 120" className="h-full w-full rotate-[135deg]"><circle cx="60" cy="60" r="52" fill="none" stroke="#262626" strokeWidth="10" strokeDasharray={`${d.arc} ${d.circumference}`} strokeLinecap="round" />
      <circle cx="60" cy="60" r="52" fill="none" stroke={b.color} strokeWidth="10" strokeDasharray={`${d.filled} ${d.circumference}`} strokeLinecap="round" /></svg>
    <div className="absolute inset-0 flex flex-col items-center justify-center"><span data-testid="score" className="text-4xl font-bold">{b.score}</span><span className="text-sm" style={{ color: b.color }}>{b.label}</span></div></div>);
}
const CARDS: { key: IssueKey; title: string; desc: string; Icon: typeof AlertTriangle }[] = [
  { key: 'weak', title: 'Weak passwords', desc: 'Easy to guess. Generate a strong replacement.', Icon: AlertTriangle },
  { key: 'reused', title: 'Reused passwords', desc: 'Same password on multiple sites.', Icon: Copy },
  { key: 'old', title: 'Old passwords', desc: 'Not changed in a long time.', Icon: Clock },
  { key: 'breached', title: 'Breached passwords', desc: 'Found in known data breaches (HIBP).', Icon: ShieldAlert },
  { key: 'missingTotp', title: 'Missing 2FA', desc: 'Logins with a URL but no one-time password saved.', Icon: Smartphone },
];
export function WatchtowerPage() {
  const t = useT();
  const cards = [...CARDS,
    { key: 'totpAvailable' as const, title: t('watchtower.totpAvailable'), desc: t('watchtower.totpAvailableDesc'), Icon: Smartphone },
    { key: 'passkeyAvailable' as const, title: t('watchtower.passkeyAvailable'), desc: t('watchtower.passkeyAvailableDesc'), Icon: KeyRound },
    { key: 'expired' as const, title: t('watchtower.expired'), desc: t('watchtower.expiredDesc'), Icon: CalendarX },
    { key: 'expiringSoon' as const, title: t('watchtower.expiringSoon'), desc: t('watchtower.expiringSoonDesc'), Icon: CalendarClock },
    { key: 'insecureUrls' as const, title: t('watchtower.insecureUrls'), desc: t('watchtower.insecureUrlsDesc'), Icon: Globe },
  ];
  const { selectItem, setRoute, setEditing } = useApp();
  // Opt-in HIBP lives in Rust-persisted Settings.hibpCheck (default off); Rust also refuses the network
  // check unless that setting is on, so this toggle is the only way to enable it.
  const settings = useSettings((st) => st.settings); const hibp = !!settings?.hibpCheck;
  useEffect(() => { if (!settings) void useSettings.getState().load(); }, [settings]);
  const setHibp = (v: boolean) => { useSettings.getState().save({ hibpCheck: v }).catch((e) => setErr(String((e as Error)?.message ?? e))); };
  const [report, setReport] = useState<WatchtowerReport | null>(null); const [titles, setTitles] = useState<Record<string, ItemSummary>>({});
  const [err, setErr] = useState<string | null>(null); const [busy, setBusy] = useState(false);
  const load = useCallback(async (h: boolean) => { setBusy(true); setErr(null);
    try { const [r, items] = await Promise.all([api.watchtower({ hibp: h }), api.listItems({ filter: { kind: 'all' } })]); setReport(r); setTitles(Object.fromEntries(items.map(i => [i.id, i]))); }
    catch (e) { setErr(String(e)); } finally { setBusy(false); } }, []);
  const ready = !!settings;
  useEffect(() => { if (ready) load(hibp); }, [hibp, ready, load]);
  const open = (id: string) => { const it = titles[id]; setRoute({ name: 'items', filter: it ? { kind: 'vault', vaultId: it.vaultId } : { kind: 'all' } }); selectItem(id); }; // setRoute clears selection
  const openEdit = (id: string) => { const it = titles[id]; setRoute({ name: 'items', filter: it ? { kind: 'vault', vaultId: it.vaultId } : { kind: 'all' } }); selectItem(id); setEditing(true); };
  return (<div className="mx-auto max-w-5xl space-y-6 p-8">
    <div className="flex items-center justify-between"><h1 className="text-2xl font-semibold">Watchtower</h1><Button variant="ghost" size="sm" disabled={busy} onClick={() => load(hibp)}><RefreshCw size={14} /> Re-check</Button></div>
    {err && <p role="alert" className="text-sm text-danger">{err}</p>}
    <div className="flex flex-wrap items-center gap-8 rounded-xl border border-border bg-panel p-6">
      {report ? <ScoreGauge score={report.score} /> : <Skeleton className="h-40 w-40 rounded-full" />}
      <div className="max-w-md flex-1 space-y-3">
        <p className="text-fg">Your security score reflects weak, reused, old and breached passwords and missing two-factor authentication.</p>
        {report && <p className="text-xs text-muted">Last checked {new Date(report.checkedAt).toLocaleString()}{report.hibpChecked ? ' · breach check included' : ' · breach check off'}</p>}
        <div className="rounded-lg border border-border p-3">
          <label className="flex items-center gap-2 text-sm font-medium"><input type="checkbox" role="switch" aria-label="Check for breached passwords" checked={hibp} onChange={e => setHibp(e.target.checked)} className="accent-accent" /> Check passwords against known breaches</label>
          <p className="mt-1 text-xs text-muted">Off by default. When on, gitpass hashes each password with SHA-1 locally and sends only the <b>first 5 characters</b> of the hash to Have I Been Pwned (k-anonymity). HIBP returns every breached hash with that prefix and the match is done on your device — your passwords and full hashes never leave this computer, and HIBP can’t tell which password you checked.</p></div></div></div>
    <div className="grid gap-4 md:grid-cols-2">{cards.map(({ key, title, desc, Icon }) => { const ids = report ? issueIds(report, key) : [];
      const off = key === 'breached' && report && !report.hibpChecked;
      return (<section key={key} aria-label={title} className="rounded-xl border border-border bg-panel p-4">
        <div className="flex items-center gap-2"><Icon size={18} className={ids.length ? (key === 'expired' ? 'text-danger' : key === 'expiringSoon' ? 'text-accent' : 'text-warn') : 'text-ok'} /><h2 className="font-semibold">{title}</h2>
          <span data-testid={`count-${key}`} className="ml-auto rounded-full bg-hover px-2 text-sm">{report ? (off ? '—' : ids.length) : '…'}</span></div>
        <p className="mt-1 text-xs text-muted">{desc}</p>
        {off ? <p className="mt-3 text-xs text-muted">Enable the breach check above to scan.</p>
          : ids.length === 0 ? (report && <p className="mt-3 flex items-center gap-1 text-xs text-ok"><ShieldCheck size={12} /> No issues</p>)
          : key === 'reused' ? <ul className="mt-3 space-y-2">{report!.reused.map((g, i) => <li key={i} className="rounded bg-bg p-2"><div className="mb-1 text-[11px] text-muted">Group {i + 1} · {g.length} items</div><div className="flex flex-wrap gap-1">{g.map(id => <ItemChip key={id} title={titles[id]?.title ?? id} onClick={() => open(id)} />)}</div></li>)}</ul>
          : key === 'insecureUrls' ? (
            <ul className="mt-3 space-y-2" data-testid="insecure-urls-list">
              {ids.map((id) => {
                const it = titles[id];
                const rawUrl = it?.url ?? '';
                const httpsUrl = rawUrl.replace(/^http:\/\//i, 'https://');
                return (
                  <li
                    key={id}
                    className="flex flex-col gap-2 rounded-lg bg-bg p-2.5 text-xs sm:flex-row sm:items-center sm:justify-between"
                    data-testid={`insecure-item-${id}`}
                  >
                    <div className="min-w-0 flex-1">
                      <button
                        type="button"
                        onClick={() => open(id)}
                        className="truncate text-left font-medium text-fg hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                      >
                        {it?.title ?? id}
                      </button>
                      {rawUrl && <div className="truncate font-mono text-[11px] text-muted">{rawUrl}</div>}
                    </div>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {httpsUrl && (
                        <a
                          href={httpsUrl}
                          target="_blank"
                          rel="noreferrer noopener"
                          className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                          aria-label={`${t('watchtower.openHttps')} for ${it?.title ?? id}`}
                        >
                          <ExternalLink size={12} className="text-accent" />
                          <span>{t('watchtower.openHttps')}</span>
                        </a>
                      )}
                      <button
                        type="button"
                        onClick={() => openEdit(id)}
                        className="inline-flex items-center gap-1 rounded bg-hover px-2 py-1 text-[11px] font-medium text-fg hover:bg-hover/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
                        aria-label={`${t('watchtower.editUrl')} for ${it?.title ?? id}`}
                      >
                        <Pencil size={12} className="text-accent" />
                        <span>{t('watchtower.editUrl')}</span>
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )
          : <div className="mt-3 flex flex-wrap gap-1">{ids.map(id => <ItemChip key={id} title={titles[id]?.title ?? id} onClick={() => open(id)} />)}</div>}
      </section>); })}</div></div>);
}
const ItemChip = ({ title, onClick }: { title: string; onClick: () => void }) => <button onClick={onClick} className="rounded-md bg-hover px-2 py-1 text-xs hover:bg-hover">{title}</button>;
