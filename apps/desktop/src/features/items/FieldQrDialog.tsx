import { useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Eye, EyeOff, Loader2 } from 'lucide-react';
import { api } from '../../api/client';
import type { Field } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { cn } from '../../lib/utils';
import { Button, Dialog, DialogContent } from '../../ui';
import { errText } from './actions';
import { buildTotpUri } from './fieldQr';

export interface FieldQrDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  ctx: { vaultId: string; itemId: string };
  itemTitle: string;
  field: Field;
  username?: string;
  triggerRef?: React.RefObject<HTMLElement | null>;
}

export function FieldQrDialog({
  open,
  onOpenChange,
  ctx,
  itemTitle,
  field,
  username,
  triggerRef,
}: FieldQrDialogProps) {
  const t = useT();
  const locked = useApp((s) => s.locked);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [rawValue, setRawValue] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);

  const isConcealed = field.kind === 'concealed' || field.kind === 'totp';

  // If the vault locks, immediately dismiss and drop sensitive QR data.
  useEffect(() => {
    if (locked && open) {
      onOpenChange(false);
    }
  }, [locked, open, onOpenChange]);

  useEffect(() => {
    const onLock = () => onOpenChange(false);
    window.addEventListener('gitpass:locked', onLock);
    return () => window.removeEventListener('gitpass:locked', onLock);
  }, [onOpenChange]);

  useEffect(() => {
    if (!open) {
      setQrDataUrl(null);
      setRawValue(null);
      setRevealed(false);
      setLoading(false);
      setError(null);
    }
  }, [open, field.id]);

  useEffect(() => {
    if (!open || locked) return;

    let cancelled = false;
    setLoading(true);
    setError(null);

    async function loadAndGenerate() {
      try {
        let val = '';
        if (field.kind === 'concealed' || field.kind === 'totp') {
          val = await api.revealField({
            vaultId: ctx.vaultId,
            itemId: ctx.itemId,
            fieldId: field.id,
          });
        } else {
          val = field.value ?? '';
        }

        if (cancelled) return;
        setRawValue(val);

        if (!val) {
          setError(t('field.errorGeneratingQr'));
          setLoading(false);
          return;
        }

        let payload = val;
        if (field.kind === 'totp') {
          payload = buildTotpUri(val, itemTitle, username);
        }

        if (payload.length > 2048) {
          throw new Error(t('field.qrTooLong'));
        }

        const url = await QRCode.toDataURL(payload, {
          margin: 2,
          width: 220,
          color: { dark: '#000000', light: '#ffffff' },
          errorCorrectionLevel: 'M',
        });

        if (cancelled) return;
        setQrDataUrl(url);
        setLoading(false);
      } catch (e) {
        if (!cancelled) {
          setError(errText(e) || t('field.errorGeneratingQr'));
          setLoading(false);
        }
      }
    }

    void loadAndGenerate();

    return () => {
      cancelled = true;
    };
  }, [open, locked, ctx.vaultId, ctx.itemId, field.id, field.kind, field.value, itemTitle, username]); // eslint-disable-line react-hooks/exhaustive-deps

  const displayMasked = isConcealed && !revealed;
  const displayValue = isConcealed
    ? (revealed ? (rawValue ?? '') : (field.masked || '••••••••••'))
    : (rawValue || field.value || '—');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={itemTitle || t('field.qrDialogTitle')}
        description={t('field.qrWarning')}
        className="w-[380px]"
        onCloseAutoFocus={(e) => {
          if (triggerRef?.current) {
            e.preventDefault();
            triggerRef.current.focus();
          }
        }}
      >
        <div className="flex flex-col items-center text-center">
          {/* White quiet-zone container for dark and light mode scanning */}
          <div
            className="rounded-xl bg-white p-3 shadow-md border border-neutral-200 inline-block"
            data-testid="field-qr-container"
          >
            {qrDataUrl ? (
              <img
                src={qrDataUrl}
                alt={t('field.qrCodeAlt', { label: field.label })}
                className="h-[220px] w-[220px] block"
                data-testid="field-qr-image"
              />
            ) : loading ? (
              <div
                className="flex h-[220px] w-[220px] items-center justify-center text-neutral-400"
                data-testid="field-qr-loading"
              >
                <Loader2 className="h-8 w-8 animate-spin" />
              </div>
            ) : (
              <div
                className="flex h-[220px] w-[220px] items-center justify-center p-4 text-xs text-danger"
                data-testid="field-qr-error"
              >
                {error || t('field.errorGeneratingQr')}
              </div>
            )}
          </div>

          {/* Item title under the code */}
          <div
            className="mt-4 flex max-w-full items-center justify-center gap-1.5 text-lg font-semibold text-fg"
            data-testid="field-qr-title"
          >
            <span className="truncate">{itemTitle}</span>
          </div>

          {/* Field label under the title */}
          <div className="text-xs font-medium text-label" data-testid="field-qr-label">
            {field.label}
          </div>

          {/* Warning line */}
          <div className="mt-1 text-xs text-muted" data-testid="field-qr-warning">
            {t('field.qrWarning')}
          </div>

          {/* Field value row (masked for concealed fields) */}
          <div
            className="mt-4 flex w-full items-center justify-between rounded-lg border border-border bg-card px-3.5 py-2 text-sm"
            data-testid="field-qr-value-row"
          >
            <div className="min-w-0 flex-1 text-left">
              <div className="text-[11px] font-medium text-label">{field.label}</div>
              <div
                className={cn('font-mono text-sm tracking-wider truncate', displayMasked && 'tracking-widest')}
                data-testid="field-qr-value"
              >
                {displayValue}
              </div>
            </div>
            {isConcealed && (
              <Button
                type="button"
                variant="ghost"
                size="iconSm"
                aria-label={revealed ? t('field.concealValue', { label: field.label }) : t('field.revealValue', { label: field.label })}
                data-testid="field-qr-reveal-btn"
                onClick={() => setRevealed(!revealed)}
              >
                {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
              </Button>
            )}
          </div>

          {/* Orange accent primary action button */}
          <div className="mt-5 flex w-full justify-end">
            <Button
              type="button"
              variant="default"
              onClick={() => onOpenChange(false)}
              data-testid="field-qr-close-btn"
              className="w-full sm:w-auto"
            >
              {t('common.close')}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
