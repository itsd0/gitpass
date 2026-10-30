import { useEffect, useRef, useState } from 'react';
import { Copy, Eye, EyeOff, ExternalLink, Link2, Maximize2, MoreHorizontal, QrCode } from 'lucide-react';
import { api } from '../../api/client';
import type { Field, ItemCategory, PasswordHistoryRef } from '../../api/types';
import { useT } from '../../i18n';
import { checkExpiry, isExpiryField } from './expiry';
import { formatRelativePasswordChanged } from './passwordAge';
import { LargeTypeDialog } from './LargeType';
import { FieldQrDialog } from './FieldQrDialog';
import { isQrSupportedField } from './fieldQr';
import { PasswordHistory } from './PasswordHistory';
import { cn } from '../../lib/utils';
import { Badge, Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Tooltip, toast } from '../../ui';
import { errText } from './actions';
import { toastFieldCopied } from '../../lib/concealedCopy';
import { groupCardNumber } from '../../lib/cardMask';
import { SafeMarkdown } from './SafeMarkdown';
import { useSettings } from '../settings/useSettings';
import { secretRef, type RefTarget } from '../../lib/secretRef';
import { SecretText } from '../../lib/secretText';

export const REVEAL_MS = 30_000;
const DOTS = '••••••••••';

interface Ctx { vaultId: string; itemId: string }

export function FieldRow({
  field,
  ctx,
  strength,
  category,
  passwordChangedAt,
  refTarget,
  itemTitle,
  username,
  passwordHistory,
  onHistoryCleared,
}: {
  field: Field;
  ctx: Ctx;
  strength?: number | null;
  category?: ItemCategory;
  passwordChangedAt?: string | null;
  /** where `gp://` references for this item's fields point; omit to hide the action */
  refTarget?: RefTarget | null;
  itemTitle?: string;
  username?: string;
  passwordHistory?: PasswordHistoryRef[];
  onHistoryCleared?: () => void;
}) {
  const t = useT();
  const [qrOpen, setQrOpen] = useState(false);
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  const qrBtnRef = useRef<HTMLButtonElement>(null);
  const lastTriggerRef = useRef<HTMLElement | null>(null);

  const isQrSupported = isQrSupportedField(field, category);

  const openQr = (trigger?: HTMLElement | null) => {
    lastTriggerRef.current = trigger || qrBtnRef.current || rowRef.current;
    setQrOpen(true);
  };

  const onContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setMenuPos({ x: e.clientX, y: e.clientY });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault();
      e.stopPropagation();
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      setMenuPos({ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 });
    }
  };

  return (
    <div
      ref={rowRef}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
      className="group relative border-b border-border px-4 py-2.5 last:border-b-0 hover:bg-hover/50"
      data-testid={`field-${field.id}`}
    >
      <div className="text-xs text-label">{field.label}</div>
      {field.kind === 'concealed' ? (
        <ConcealedValue
          field={field}
          ctx={ctx}
          strength={strength}
          category={category}
          passwordChangedAt={passwordChangedAt}
          refTarget={refTarget}
          isQrSupported={isQrSupported}
          onOpenQr={openQr}
          passwordHistory={passwordHistory}
          onHistoryCleared={onHistoryCleared}
          />
      ) : field.kind === 'totp' ? (
        <TotpValue
          ctx={ctx}
          field={field}
          refTarget={refTarget}
          isQrSupported={isQrSupported}
          onOpenQr={openQr}
          />
      ) : (
        <PlainValue
          field={field}
          ctx={ctx}
          category={category}
          refTarget={refTarget}
          isQrSupported={isQrSupported}
          onOpenQr={openQr}
          />
      )}

      {menuPos && (
        <DropdownMenu open onOpenChange={(o) => !o && setMenuPos(null)}>
          <DropdownMenuTrigger asChild>
            <span
              aria-hidden="true"
              style={{ position: 'fixed', left: menuPos.x, top: menuPos.y, width: 0, height: 0 }}
            />
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="start"
            side="bottom"
            aria-label={t('field.menuLabel', { label: field.label })}
            data-testid="field-context-menu"
            className="min-w-[170px]"
            onCloseAutoFocus={(e) => {
              e.preventDefault();
              setMenuPos(null);
            }}
          >
            <DropdownMenuItem
              data-testid="ctx-menu-copy"
              onSelect={() => {
                void api.copyField({ ...ctx, fieldId: field.id }).then(
                  () => toastFieldCopied(field, category),
                  (e) => toast(errText(e), 'danger')
                );
              }}
            >
              <Copy size={14} />
              {t('common.copy')}
            </DropdownMenuItem>
            {refTarget && (
              <DropdownMenuItem
                data-testid="ctx-menu-copy-ref"
                onSelect={() => {
                  const ref = secretRef(refTarget, field);
                  if (navigator.clipboard) {
                    navigator.clipboard.writeText(ref).then(
                      () => toast(t('ref.copied')),
                      () => toast(t('ref.failed'), 'danger')
                    );
                  }
                }}
              >
                <Link2 size={14} />
                {t('ref.copy')}
              </DropdownMenuItem>
            )}
            {isQrSupported && (
              <DropdownMenuItem
                data-testid="menu-show-qr"
                onSelect={() => openQr(rowRef.current)}
              >
                <QrCode size={14} />
                {t('field.showQrCode')}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      {qrOpen && (
        <FieldQrDialog
          open={qrOpen}
          onOpenChange={setQrOpen}
          ctx={ctx}
          itemTitle={itemTitle || 'gitpass'}
          field={field}
          username={username}
          triggerRef={lastTriggerRef}
        />
      )}
    </div>
  );
}

function CopyBtn({ ctx, field, category }: { ctx: Ctx; field: Field; category?: ItemCategory }) {
  const copyName = field.kind === 'totp' ? 'one-time password' : field.label;
  return (
    <Tooltip content="Copy">
      <Button variant="ghost" size="iconSm" aria-label={`Copy ${copyName}`} onClick={() => api.copyField({ ...ctx, fieldId: field.id }).then(() => toastFieldCopied(field, category), (e) => toast(errText(e), 'danger'))}><Copy size={13} /></Button>
    </Tooltip>
  );
}

/** Copies `gp://vault/item/field` (a name, not a secret): plain clipboard write, no concealed marker, no auto-clear. */
function RefBtn({ field, target }: { field: Field; target?: RefTarget | null }) {
  const t = useT();
  if (!target) return null;
  const copy = () => {
    const ref = secretRef(target, field);
    const done = () => toast(t('ref.copied'));
    const fail = () => toast(t('ref.failed'), 'danger');
    if (!navigator.clipboard) { fail(); return; }
    navigator.clipboard.writeText(ref).then(done, fail);
  };
  return (
    <Tooltip content={t('ref.copy')}>
      <Button variant="ghost" size="iconSm" aria-label={t('ref.copyField', { label: field.label })} data-testid={`copy-ref-${field.id}`} onClick={copy}><Link2 size={13} /></Button>
    </Tooltip>
  );
}

function FieldActionMenu({
  field,
  ctx,
  category,
  refTarget,
  isQrSupported,
  onShowLargeType,
  onShowQr,
}: {
  field: Field;
  ctx: Ctx;
  category?: ItemCategory;
  refTarget?: RefTarget | null;
  isQrSupported: boolean;
  onShowLargeType?: () => void;
  onShowQr?: () => void;
}) {
  const t = useT();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="iconSm"
          aria-label={t('field.moreActions', { label: field.label })}
          data-testid={`field-more-actions-${field.id}`}
        >
          <MoreHorizontal size={13} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-[170px]" data-testid="field-action-menu">
        <DropdownMenuItem
          data-testid="menu-copy-field"
          onSelect={() => {
            void api.copyField({ ...ctx, fieldId: field.id }).then(
              () => toastFieldCopied(field, category),
              (e) => toast(errText(e), 'danger')
            );
          }}
        >
          <Copy size={14} />
          {t('common.copy')}
        </DropdownMenuItem>
        {refTarget && (
          <DropdownMenuItem
            data-testid="menu-copy-ref"
            onSelect={() => {
              const ref = secretRef(refTarget, field);
              if (navigator.clipboard) {
                navigator.clipboard.writeText(ref).then(
                  () => toast(t('ref.copied')),
                  () => toast(t('ref.failed'), 'danger')
                );
              }
            }}
          >
            <Link2 size={14} />
            {t('ref.copy')}
          </DropdownMenuItem>
        )}
        {onShowLargeType && (
          <DropdownMenuItem data-testid="menu-show-large-type" onSelect={onShowLargeType}>
            <Maximize2 size={14} />
            {t('largetype.action')}
          </DropdownMenuItem>
        )}
        {isQrSupported && onShowQr && (
          <DropdownMenuItem data-testid="menu-show-qr" onSelect={onShowQr}>
            <QrCode size={14} />
            {t('field.showQrCode')}
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function PlainValue({
  field,
  ctx,
  category,
  refTarget,
  isQrSupported,
  onOpenQr,
  }: {
  field: Field;
  ctx: Ctx;
  category?: ItemCategory;
  refTarget?: RefTarget | null;
  isQrSupported: boolean;
  onOpenQr?: (trigger?: HTMLElement | null) => void;
}) {
  const t = useT();
  const qrBtnRef = useRef<HTMLButtonElement>(null);
  const renderNotesMarkdown = useSettings((s) => s.settings?.renderNotesMarkdown ?? true);
  const v = field.value ?? '';
  const isExp = isExpiryField(field, category);
  const expiry = isExp && v ? checkExpiry(v, field.kind, undefined, t) : null;
  const isNoteField = field.kind === 'multiline' && (
    category === 'secure_note' ||
    field.id === 'note' ||
    field.id === 'notes' ||
    /note/i.test(field.label)
  );

  return (
    <div className="flex items-start gap-2">
      <div className={cn('min-w-0 flex-1 break-words text-[14px]', field.kind === 'multiline' && !isNoteField && 'whitespace-pre-wrap font-mono text-xs', field.kind === 'multiline' && isNoteField && !renderNotesMarkdown && 'whitespace-pre-wrap font-mono text-xs', field.kind === 'url' && 'text-accent')}>
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
          ) : isNoteField && renderNotesMarkdown ? (
            <SafeMarkdown content={v} />
          ) : (
            v
          )
        ) : (
          <span className="text-muted">—</span>
        )}
      </div>
      <div className="flex opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        {field.kind === 'url' && v && <Tooltip content={t('field.open')}><a className="inline-flex h-6 w-6 items-center justify-center rounded hover:bg-hover" href={v} target="_blank" rel="noreferrer noopener" aria-label={t('field.openLink')}><ExternalLink size={13} /></a></Tooltip>}
        {isQrSupported && Boolean(v) && (
          <Tooltip content={t('field.showQrCode')}>
            <Button
              ref={qrBtnRef}
              variant="ghost"
              size="iconSm"
              aria-label={t('field.showQrAria', { label: field.label })}
              data-testid="show-qr-trigger"
              onClick={() => onOpenQr?.(qrBtnRef?.current)}
            >
              <QrCode size={13} />
            </Button>
          </Tooltip>
        )}
        {v && <CopyBtn ctx={ctx} field={field} category={category} />}
        {v && <RefBtn field={field} target={refTarget} />}
        {v && (
          <FieldActionMenu
            field={field}
            ctx={ctx}
            category={category}
            refTarget={refTarget}
            isQrSupported={Boolean(isQrSupported && v)}
            onShowQr={() => onOpenQr?.(qrBtnRef?.current)}
          />
        )}
      </div>
    </div>
  );
}

/** Concealed values are never in the payload; only fetched via revealField, auto-hidden after 30s. */
export function ConcealedValue({
  field,
  ctx,
  strength,
  category,
  passwordChangedAt,
  refTarget,
  isQrSupported = false,
  onOpenQr,
  passwordHistory,
  onHistoryCleared,
  }: {
  field: Field;
  ctx: Ctx;
  strength?: number | null;
  category?: ItemCategory;
  passwordChangedAt?: string | null;
  refTarget?: RefTarget | null;
  isQrSupported?: boolean;
  onOpenQr?: (trigger?: HTMLElement | null) => void;
  passwordHistory?: PasswordHistoryRef[];
  onHistoryCleared?: () => void;
}) {
  const t = useT();
  const [revealed, setRevealed] = useState<string | null>(null);
  const [largeTypeOpen, setLargeTypeOpen] = useState(false);
  const [largeTypeValue, setLargeTypeValue] = useState<string | null>(null);
  const qrBtnRef = useRef<HTMLButtonElement>(null);
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

  const isApi = category === 'api_credential';
  const shouldGroup = !isApi && Boolean(field.masked);

  return (
    <div
      ref={containerRef}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="outline-none focus-within:ring-1 focus-within:ring-accent/40 rounded px-1 -mx-1"
      data-testid={`concealed-field-${field.id}`}
    >
      <div className="flex items-center gap-2">
        <div className={cn('min-w-0 flex-1 break-all font-mono text-[14px]', revealed === null && 'tracking-widest')} data-testid="concealed-value">{revealed === null ? (field.masked || DOTS) : (shouldGroup ? groupCardNumber(revealed) : <SecretText value={revealed} />)}</div>
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
          {isQrSupported && (
            <Tooltip content={t('field.showQrCode')}>
              <Button
                ref={qrBtnRef}
                variant="ghost"
                size="iconSm"
                aria-label={t('field.showQrAria', { label: field.label })}
                data-testid="show-qr-trigger"
                onClick={() => onOpenQr?.(qrBtnRef?.current)}
              >
                <QrCode size={13} />
              </Button>
            </Tooltip>
          )}
          <Tooltip content={revealed === null ? 'Reveal' : 'Conceal'}>
            <Button variant="ghost" size="iconSm" aria-label={revealed === null ? `Reveal ${field.label}` : `Conceal ${field.label}`} onClick={toggle}>{revealed === null ? <Eye size={13} /> : <EyeOff size={13} />}</Button>
          </Tooltip>
          <CopyBtn ctx={ctx} field={field} category={category} />
          <RefBtn field={field} target={refTarget} />
          <FieldActionMenu
            field={field}
            ctx={ctx}
            category={category}
            refTarget={refTarget}
            isQrSupported={Boolean(isQrSupported)}
            onShowLargeType={openLargeType}
            onShowQr={() => onOpenQr?.(qrBtnRef?.current)}
          />
        </div>
      </div>
      {strength != null && /password/i.test(field.label) && <StrengthMeter score={strength} />}
      {passwordChangedAt && /password/i.test(field.label) && (
        <div className="mt-1 text-xs text-muted" data-testid="password-changed-at">
          {formatRelativePasswordChanged(passwordChangedAt, undefined, t)}
        </div>
      )}
      {passwordHistory && passwordHistory.length > 0 && /password/i.test(field.label) && (
        <PasswordHistory history={passwordHistory} ctx={ctx} onCleared={onHistoryCleared} />
      )}
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

const STRENGTH = [['strength.terrible', 'bg-danger'], ['strength.weak', 'bg-danger'], ['strength.fair', 'bg-warn'], ['strength.good', 'bg-ok'], ['strength.excellent', 'bg-ok']] as const;
/** `score` is the backend's 0-100 strength (gui-core `strength`), bucketed into 5 bars. */
export function StrengthMeter({ score }: { score: number }) {
  const t = useT();
  const s = Math.max(0, Math.min(4, Math.floor(score / 20)));
  const label = t(STRENGTH[s][0]);
  return (
    <div className="mt-1.5 flex items-center gap-2" data-testid="strength-meter" role="meter" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.max(0, Math.min(100, score))} aria-label={t('strength.label', { label })}>
      <div className="flex w-32 gap-0.5">{[0, 1, 2, 3, 4].map((i) => <div key={i} className={cn('h-1 flex-1 rounded-full', i <= s ? STRENGTH[s][1] : 'bg-active')} />)}</div>
      <span className="text-xs text-muted">{label}</span>
    </div>
  );
}

export function TotpRing({ remaining, period, size = 18 }: { remaining: number; period: number; size?: number }) {
  const r = size / 2 - 2; const c = 2 * Math.PI * r; const frac = Math.max(0, Math.min(1, remaining / period));
  return (
    <svg width={size} height={size} className="-rotate-90" aria-hidden="true">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="rgb(var(--active))" strokeWidth="2.5" />
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={remaining <= 5 ? 'rgb(var(--danger))' : 'rgb(var(--accent))'} strokeWidth="2.5" strokeDasharray={c} strokeDashoffset={c * (1 - frac)} strokeLinecap="round" style={{ transition: 'stroke-dashoffset 1s linear' }} />
    </svg>
  );
}

function TotpValue({
  ctx,
  field,
  refTarget,
  isQrSupported = true,
  onOpenQr,
  }: {
  ctx: Ctx;
  field: Field;
  refTarget?: RefTarget | null;
  isQrSupported?: boolean;
  onOpenQr?: (trigger?: HTMLElement | null) => void;
}) {
  const t = useT();
  const qrBtnRef = useRef<HTMLButtonElement>(null);
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
        {state ? <><span aria-label={`${state.code} ${t('totp.remaining', { n: state.remaining })}`}>{state.code.slice(0, 3)} {state.code.slice(3)}</span><TotpRing remaining={state.remaining} period={state.period} /><span className="text-xs text-muted" aria-hidden="true">{state.remaining}s</span></> : '··· ···'}
      </div>
      <div className="flex opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
        {isQrSupported && (
          <Tooltip content={t('field.showQrCode')}>
            <Button
              ref={qrBtnRef}
              variant="ghost"
              size="iconSm"
              aria-label={t('field.showQrAria', { label: field.label })}
              data-testid="show-qr-trigger"
              onClick={() => onOpenQr?.(qrBtnRef?.current)}
            >
              <QrCode size={13} />
            </Button>
          </Tooltip>
        )}
        <CopyBtn ctx={ctx} field={field} />
        <RefBtn field={field} target={refTarget} />
        <FieldActionMenu
          field={field}
          ctx={ctx}
          refTarget={refTarget}
          isQrSupported={Boolean(isQrSupported)}
          onShowQr={() => onOpenQr?.(qrBtnRef?.current)}
        />
      </div>
    </div>
  );
}
