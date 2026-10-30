import { useState } from 'react';
import { useT, useI18n } from '../../i18n';
import { cn } from '../../lib/utils';
import { daysBetween, inRange, weeklyDelta, type HistoryRange, type ScorePoint } from './history';

const W = 280;
const H = 56;
const PAD = 4;

/** Sparkline of the daily security score with a 30/90 day toggle and a weekly delta. */
export function ScoreHistory({ points, now }: { points: ScorePoint[]; now?: Date }) {
  const t = useT();
  const lang = useI18n((s) => s.lang);
  const [range, setRange] = useState<HistoryRange>(30);
  const [active, setActive] = useState<number | null>(null);
  const clock = now ?? new Date();
  const shown = inRange(points, range, clock);
  const fmt = (date: string) => new Date(`${date}T12:00:00`).toLocaleDateString(lang, { month: 'short', day: 'numeric' });

  const toggle = (
    <div role="group" aria-label={t('watchtower.history.range')} className="inline-flex rounded-md border border-border p-0.5 text-xs">
      {([30, 90] as const).map((r) => (
        <button
          key={r}
          type="button"
          aria-pressed={range === r}
          onClick={() => { setRange(r); setActive(null); }}
          className={cn('rounded px-2 py-0.5 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent', range === r ? 'bg-accent/15 text-accent' : 'text-muted hover:text-fg')}
        >
          {t('watchtower.history.days', { n: r })}
        </button>
      ))}
    </div>
  );

  if (points.length < 2 || shown.length < 2) {
    return (
      <div className="space-y-2" data-testid="score-history">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold">{t('watchtower.history.title')}</h2>
          {toggle}
        </div>
        <p className="text-xs text-muted" data-testid="score-history-empty">{t('watchtower.history.empty')}</p>
      </div>
    );
  }

  const first = shown[0]!;
  const last = shown[shown.length - 1]!;
  const span = Math.max(1, daysBetween(first.date, last.date));
  const xy = shown.map((p) => ({
    x: PAD + ((W - 2 * PAD) * daysBetween(first.date, p.date)) / span,
    y: PAD + (H - 2 * PAD) * (1 - p.score / 100),
  }));
  const line = xy.map((q, i) => `${i ? 'L' : 'M'}${q.x.toFixed(1)} ${q.y.toFixed(1)}`).join(' ');
  const area = `${line} L${xy[xy.length - 1]!.x.toFixed(1)} ${H - PAD} L${xy[0]!.x.toFixed(1)} ${H - PAD} Z`;
  const idx = active ?? shown.length - 1;
  const cur = shown[idx]!;
  const delta = weeklyDelta(points, clock);

  const onKey = (e: React.KeyboardEvent) => {
    const step = e.key === 'ArrowRight' || e.key === 'ArrowUp' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowDown' ? -1 : 0;
    if (step) { e.preventDefault(); setActive(Math.min(shown.length - 1, Math.max(0, idx + step))); }
    else if (e.key === 'Home') { e.preventDefault(); setActive(0); }
    else if (e.key === 'End') { e.preventDefault(); setActive(shown.length - 1); }
    else if (e.key === 'Escape') setActive(null);
  };
  const onMove = (e: React.MouseEvent<SVGSVGElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - box.left) / (box.width || W)) * W;
    let best = 0;
    xy.forEach((q, i) => { if (Math.abs(q.x - x) < Math.abs(xy[best]!.x - x)) best = i; });
    setActive(best);
  };

  let deltaText: string;
  let deltaClass = 'text-muted';
  if (!delta || delta.delta === 0) deltaText = t('watchtower.history.noChange');
  else {
    const n = Math.abs(delta.delta);
    const up = delta.delta > 0;
    deltaClass = up ? 'text-ok' : 'text-danger';
    deltaText = delta.week
      ? t(up ? 'watchtower.history.upWeek' : 'watchtower.history.downWeek', { n })
      : t(up ? 'watchtower.history.upSince' : 'watchtower.history.downSince', { n, date: fmt(delta.since) });
  }
  const summary = t('watchtower.history.summary', { from: first.score, to: last.score, n: range });

  return (
    <div className="space-y-2" data-testid="score-history">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold">{t('watchtower.history.title')}</h2>
        {toggle}
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative">
          <svg
            viewBox={`0 0 ${W} ${H}`}
            width={W}
            height={H}
            tabIndex={0}
            role="group"
            aria-label={summary}
            aria-roledescription="chart"
            onKeyDown={onKey}
            onBlur={() => setActive(null)}
            onMouseMove={onMove}
            onMouseLeave={() => setActive(null)}
            className="rounded focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent"
            data-testid="score-history-chart"
          >
            <path d={area} className="fill-accent/15" />
            <path d={line} fill="none" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" className="stroke-accent" />
            <circle cx={xy[idx]!.x} cy={xy[idx]!.y} r={3.5} className="fill-accent stroke-panel" strokeWidth={1.5} />
          </svg>
          <p
            className="pointer-events-none mt-0.5 text-[11px] text-muted"
            aria-live="polite"
            data-testid="score-history-tooltip"
          >
            {t('watchtower.history.point', { date: fmt(cur.date), score: cur.score })}
          </p>
        </div>
        <p className={cn('text-sm font-medium', deltaClass)} data-testid="score-history-delta">{deltaText}</p>
      </div>
    </div>
  );
}
