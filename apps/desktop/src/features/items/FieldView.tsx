import { useEffect, useRef, useState } from 'react';
import { Copy, Eye, EyeOff, ExternalLink, Maximize2 } from 'lucide-react';
import { api } from '../../api/client';
import type { Field, ItemCategory } from '../../api/types';
import { useT } from '../../i18n';
import { checkExpiry, isExpiryField } from './expiry';
import { LargeTypeDialog } from './LargeType';
import { cn } from '../../lib/utils';
import { Badge, Button, Tooltip, toast } from '../../ui';
import { errText } from './actions';
import { groupCardNumber } from '../../lib/cardMask';

export const REVEAL_MS = 30_000;
const DOTS = '••••••••••';

interface Ctx { vaultId: string; itemId: string }

export function FieldRow({ field, ctx, strength, category }: { field: Field; ctx: Ctx; strength?: number | null; category?: ItemCategory }) {
  return (
    <div className="group relative border-b border-border px-4 py-2.5 last:border-b-0 hover:bg-hover/50" data-testid={`field-${field.id}`}>
      <div className="text-xs text-label">{field.label}</div>
      {field.kind === 'concealed' ? <ConcealedValue field={field} ctx={ctx} strength={strength} />
        : field.kind === 'totp' ? <TotpValue ctx={ctx} fieldId={field.id} />
        : <PlainValue field={field} ctx={ctx} category={category} />}
    </div>
  );
}

function CopyBtn({ ctx, fieldId, label }: { ctx: Ctx; fieldId: string; label: string }) {
  return (
    <Tooltip content="Copy">
      <Button variant="ghost" size="iconSm" aria-label={`Copy ${label}`} onClick={() => api.copyField({ ...ctx, fieldId }).then(() => toast(`Copied ${label}`), (e) => toast(errText(e), 'danger'))}><Copy size={13} /></Button>
    </Tooltip>
  );
}

function PlainValue({ field, ctx, category }: { field: Field; ctx: Ctx; category?: ItemCategory }) {
  const t = useT();
  const v = field.value ?? '';
  const isExp = isExpiryField(field, category);
  const expiry = isExp && v ? checkExpiry(v, field.kind, undefined, t) : null;

  return (
    <div className="flex items-start gap-2">
      <div className={cn('min-w-0 flex-1 break-words text-[14px]', field.kind === 'multiline' && 'whitespace-pre-wrap font-mono text-xs', field.kind === 'url' && 'text-accent')}>
        {v ? (
          expiry ? (
            <div className="flex flex-wrap items-center gap-2">
              <span>{v}</span>
              <Badge
                tone={expiry.status === 'expired' ? 'danger' : 'warn'}
                data-testid="expiry-pill"
                aria-label={t(expiry.status === 'expired' ? 'expiry.expired' : 'expiry.expiresSoon')}
              >
                {t(expiry.status === 'expired' ? 'expiry.expired' : 'expiry.expiresSoon')}
              </Badge>
              <span className="text-xs text-muted" data-testid="expiry-hint">{expiry.relativeHint}</span>
            </div>
          ) : (
            v
          )
        ) : (
          <span className="text-muted">—</span>
        )}
      </div>
      <div className="flex opacity-0 transition-opacity group-hover:opacity-100">
        {field.kind === 'url' && v && <Tooltip content="Open"><a className="inline-flex h-6 w-6 items-center justify-center rounded hover:bg-hover" href={v} target="_blank" rel="noreferrer noopener" aria-label="Open link"><ExternalLink size={13} /></a></Tooltip>}
        {v && <CopyBtn ctx={ctx} fieldId={field.id} label={field.label} />}
      </div>
    </div>
  );
}

/** Concealed values are never in the payload; only fetched via revealField, auto-hidden after 30s. */
export function ConcealedValue({ field, ctx, strength }: { field: Field; ctx: Ctx; strength?: number | null }) {
  const t = useT();
  const [revealed, setRevealed] = useState<string | null>(null);
  const [largeTypeOpen, setLargeTypeOpen] = useState(false);
  const [largeTypeValue, setLargeTypeValue] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const containerRef = useRef<HTMLDivElement>(null);
  const largeTypeBtnRef = useRef<HTMLButtonElement>(null);
  const lastFocusedRef = useRef<HTMLElement | null>(null);
  const wasOpenRef = useRef(false);
  useEffect(() => {
    if (wasOpenRef.current && !largeTypeOpen) {
      lastFocusedRef.current?.focus();
    }
    wasOpenRef.current = largeTypeOpen;
  }, [largeTypeOpen]);

  useEffect(() => () => { clearTimeout(timer.current); }, []);
  useEffect(() => { setRevealed(null); setLargeTypeOpen(false); }, [ctx.itemId, field.id]);

  async function toggle() {
    if (revealed !== null) { setRevealed(null); return; }
    try {
      const v = await api.revealField({ ...ctx, fieldId: field.id });
      setRevealed(v); clearTimeout(timer.current); timer.current = setTimeout(() => setRevealed(null), REVEAL_MS);
    } catch (e) { toast(errText(e), 'danger'); }
  }

  async function openLargeType() {
    lastFocusedRef.current = (document.activeElement && document.activeElement !== document.body ? document.activeElement as HTMLElement : null) || largeTypeBtnRef.current || containerRef.current;
    
    let v = revealed;
    if (v === null) {
      try {
        v = await api.revealField({ ...ctx, fieldId: field.id });
      } catch (e) {
        toast(errText(e), 'danger');
        return;
      }
    }
    setLargeTypeValue(v);
    setLargeTypeOpen(true);
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.altKey && (e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'l') {
      e.preventDefault();
      e.stopPropagation();
      void openLargeType();
    }
  };

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="outline-none focus-within:ring-1 focus-within:ring-accent/40 rounded px-1 -mx-1"
      data-testid={`concealed-field-${field.id}`}
    >
      <div className="flex items-center gap-2">
        <div className={cn('min-w-0 flex-1 break-all font-mono text-[14px]', revealed === null && 'tracking-widest')} data-testid="concealed-value">{revealed === null ? (field.masked || DOTS) : field.masked ? groupCardNumber(revealed) : revealed}</div>
        <div className="flex opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
          <Tooltip content={t('largetype.action')}>
            <Button
              ref={largeTypeBtnRef}
              variant="ghost"
              size="iconSm"
              aria-label={t('largetype.ariaField', { label: field.label })}
              data-testid="large-type-trigger"
              onClick={() => void openLargeType()}
            >
              <Maximize2 size={13} />
            </Button>
          </Tooltip>
          <Tooltip content={revealed === null ? 'Reveal' : 'Conceal'}>
            <Button variant="ghost" size="iconSm" aria-label={revealed === null ? `Reveal ${field.label}` : `Conceal ${field.label}`} onClick={toggle}>{revealed === null ? <Eye size={13} /> : <EyeOff size={13} />}</Button>
          </Tooltip>
          <CopyBtn ctx={ctx} fieldId={field.id} label={field.label} />
        </div>
      </div>
      {strength != null && /password/i.test(field.label) && <StrengthMeter score={strength} />}
      {largeTypeOpen && largeTypeValue !== null && (
        <LargeTypeDialog
          open={largeTypeOpen}
          onOpenChange={setLargeTypeOpen}
          title={field.label}
          value={largeTypeValue}
          triggerRef={lastFocusedRef}
        />
      )}
    </div>
  );
}

const STRENGTH = [['Terrible', 'bg-danger'], ['Weak', 'bg-danger'], ['Fair', 'bg-warn'], ['Good', 'bg-ok'], ['Excellent', 'bg-ok']] as const;
/** `score` is the backend's 0-100 strength (gui-core `strength`), bucketed into 5 bars. */
export function StrengthMeter({ score }: { score: number }) {
  const s = Math.max(0, Math.min(4, Math.floor(score / 20)));
  return (
    <div className="mt-1.5 flex items-center gap-2" data-testid="strength-meter">
      <div className="flex w-32 gap-0.5">{[0, 1, 2, 3, 4].map((i) => <div key={i} className={cn('h-1 flex-1 rounded-full', i <= s ? STRENGTH[s][1] : 'bg-active')} />)}</div>
      <span className="text-xs text-muted">{STRENGTH[s][0]}</span>
    </div>
  );
}

export function TotpRing({ remaining, period, size = 18 }: { remaining: number; period: number; size?: number }) {
  const r = size / 2 - 2; const c = 2 * Math.PI * r; const frac = Math.max(0, Math.min(1, remaining / period));
  return (
    <svg width={size} height={size} className="-rotate-90" aria-label={`${remaining} seconds remaining`}>
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgb(var(--active))" strokeWidth="2.5" />
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={remaining <= 5 ? 'rgb(var(--danger))' : 'rgb(var(--accent))'} strokeWidth="2.5" strokeDasharray={c} strokeDashoffset={c * (1 - frac)} strokeLinecap="round" style={{ transition: 'stroke-dashoffset 1s linear' }} />
    </svg>
  );
}

function TotpValue({ ctx, fieldId }: { ctx: Ctx; fieldId: string }) {
  const [state, setState] = useState<{ code: string; remaining: number; period: number } | null>(null);
  useEffect(() => {
    let on = true; let left = 0;
    const fetch = () => api.totpNow(ctx).then((t) => { if (on) { setState(t); left = t.remaining; } }).catch(() => {});
    void fetch();
    const iv = setInterval(() => { left -= 1; if (left <= 0) void fetch(); else setState((s) => (s ? { ...s, remaining: left } : s)); }, 1000);
    return () => { on = false; clearInterval(iv); };
  }, [ctx.vaultId, ctx.itemId]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="flex items-center gap-2">
      <div className="flex flex-1 items-center gap-2 font-mono text-[15px] tracking-wider">
        {state ? <>{state.code.slice(0, 3)} {state.code.slice(3)}<TotpRing remaining={state.remaining} period={state.period} /><span className="text-xs text-muted">{state.remaining}s</span></> : '··· ···'}
      </div>
      <div className="flex opacity-0 group-hover:opacity-100"><CopyBtn ctx={ctx} fieldId={fieldId} label="one-time password" /></div>
    </div>
  );
}
