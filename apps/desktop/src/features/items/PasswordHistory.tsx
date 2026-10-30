import { useEffect, useRef, useState } from 'react';
import { ChevronRight, Copy, Eye, EyeOff } from 'lucide-react';
import { api } from '../../api/client';
import type { PasswordHistoryRef } from '../../api/types';
import { useT } from '../../i18n';
import { cn, fmtDateLong } from '../../lib/utils';
import { Button, Dialog, DialogContent, Tooltip, toast } from '../../ui';
import { toastConcealedCopied } from '../../lib/concealedCopy';
import { errText } from './actions';
import { SecretText } from '../../lib/secretText';
import { REVEAL_MS } from './FieldView';

const DOTS = '••••••••••';

interface PasswordHistoryProps {
  history?: PasswordHistoryRef[];
  ctx: { vaultId: string; itemId: string };
  onCleared?: () => void;
}

export function PasswordHistory({ history, ctx, onCleared }: PasswordHistoryProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [revealed, setRevealed] = useState<Record<number, string>>({});
  const [confirmClear, setConfirmClear] = useState(false);
  const timers = useRef<Record<number, ReturnType<typeof setTimeout>>>({});

  useEffect(() => {
    return () => {
      Object.values(timers.current).forEach((timer) => clearTimeout(timer));
    };
  }, []);

  // Reset revealed state if item or history changes
  useEffect(() => {
    setRevealed({});
    Object.values(timers.current).forEach((timer) => clearTimeout(timer));
    timers.current = {};
  }, [ctx.itemId, history?.length]);

  if (!history || history.length === 0) {
    return null;
  }

  async function toggleReveal(index: number) {
    if (revealed[index] !== undefined) {
      if (timers.current[index]) {
        clearTimeout(timers.current[index]);
        delete timers.current[index];
      }
      setRevealed((prev) => {
        const next = { ...prev };
        delete next[index];
        return next;
      });
      return;
    }

    try {
      const val = await api.revealField({
        vaultId: ctx.vaultId,
        itemId: ctx.itemId,
        fieldId: `password_history:${index}`,
      });
      setRevealed((prev) => ({ ...prev, [index]: val }));
      if (timers.current[index]) {
        clearTimeout(timers.current[index]);
      }
      timers.current[index] = setTimeout(() => {
        setRevealed((prev) => {
          const next = { ...prev };
          delete next[index];
          return next;
        });
        delete timers.current[index];
      }, REVEAL_MS);
    } catch (e) {
      toast(errText(e), 'danger');
    }
  }

  async function copyPassword(index: number) {
    try {
      await api.copyField({
        vaultId: ctx.vaultId,
        itemId: ctx.itemId,
        fieldId: `password_history:${index}`,
      });
      toastConcealedCopied(t('itemKeys.passwordCopied'));
    } catch (e) {
      toast(errText(e), 'danger');
    }
  }

  async function handleClearHistory() {
    try {
      await api.clearPasswordHistory({
        vaultId: ctx.vaultId,
        itemId: ctx.itemId,
      });
      setConfirmClear(false);
      setOpen(false);
      toast(t('detail.clearHistoryToast'));
      onCleared?.();
    } catch (e) {
      toast(errText(e), 'danger');
    }
  }

  return (
    <div className="mt-2 text-xs" data-testid="password-history">
      <button
        type="button"
        className="flex items-center gap-1.5 text-accent hover:text-accent/80 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent rounded px-1 -mx-1"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        data-testid="password-history-toggle"
      >
        <ChevronRight size={13} className={cn('transition-transform', open && 'rotate-90')} />
        {t('detail.passwordHistory', { count: history.length })}
      </button>

      {open && (
        <div className="mt-2 space-y-2 rounded border border-border bg-card/40 p-2.5">
          <div className="space-y-2">
            {history.map((entry, idx) => {
              const isRevealed = revealed[idx] !== undefined;
              return (
                <div
                  key={idx}
                  className="flex items-center justify-between gap-2 border-b border-border/40 pb-1.5 last:border-b-0 last:pb-0"
                  data-testid={`password-history-row-${idx}`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-[11px] text-muted">
                      {t('detail.passwordHistoryReplaced', { date: fmtDateLong(entry.replacedAt) })}
                    </div>
                    <div
                      className={cn('font-mono text-xs text-fg', !isRevealed && 'tracking-widest')}
                      data-testid={`history-value-${idx}`}
                    >
                      {isRevealed ? <SecretText value={revealed[idx]} /> : DOTS}
                    </div>
                  </div>
                  <div className="flex items-center gap-0.5">
                    <Tooltip content={isRevealed ? 'Conceal' : 'Reveal'}>
                      <Button
                        variant="ghost"
                        size="iconSm"
                        aria-label={isRevealed ? 'Conceal historical password' : 'Reveal historical password'}
                        data-testid={`history-reveal-${idx}`}
                        onClick={() => void toggleReveal(idx)}
                      >
                        {isRevealed ? <EyeOff size={13} /> : <Eye size={13} />}
                      </Button>
                    </Tooltip>
                    <Tooltip content="Copy">
                      <Button
                        variant="ghost"
                        size="iconSm"
                        aria-label="Copy historical password"
                        data-testid={`history-copy-${idx}`}
                        onClick={() => void copyPassword(idx)}
                      >
                        <Copy size={13} />
                      </Button>
                    </Tooltip>
                  </div>
                </div>
              );
            })}
          </div>

          <div className="pt-1 border-t border-border/40">
            <button
              type="button"
              className="text-[11px] text-danger hover:underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-danger rounded px-1 -mx-1"
              onClick={() => setConfirmClear(true)}
              data-testid="clear-password-history-btn"
            >
              {t('detail.clearHistory')}
            </button>
          </div>
        </div>
      )}

      {confirmClear && (
        <Dialog open={confirmClear} onOpenChange={setConfirmClear}>
          <DialogContent
            title={t('detail.clearHistoryTitle')}
            description={t('detail.clearHistoryDesc')}
          >
            <div className="flex justify-end gap-2 mt-4">
              <Button
                variant="ghost"
                onClick={() => setConfirmClear(false)}
                data-testid="cancel-clear-history-btn"
              >
                {t('common.cancel')}
              </Button>
              <Button
                variant="danger"
                onClick={() => void handleClearHistory()}
                data-testid="confirm-clear-history-btn"
              >
                {t('detail.clearHistoryConfirm')}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
    </div>
  );
}
