import { useEffect, useState } from 'react';
import QRCode from 'qrcode';
import { Eye, EyeOff, Loader2, Wifi } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import { cn } from '../../lib/utils';
import { Button, Dialog, DialogContent } from '../../ui';
import { errText } from './actions';
import { buildWifiPayload, extractWifiFields } from './wifiQr';

export interface WifiQrDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  detail: ItemDetail;
  triggerRef?: React.RefObject<HTMLElement | null>;
}

export function WifiQrDialog({ open, onOpenChange, detail, triggerRef }: WifiQrDialogProps) {
  const t = useT();
  const locked = useApp((s) => s.locked);
  const wifi = extractWifiFields(detail);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [password, setPassword] = useState<string | null>(null);
  const [revealed, setRevealed] = useState(false);
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);

  // If the vault locks or dialog closes, immediately dismiss and drop sensitive QR data.
  useEffect(() => {
    if (locked && open) {
      onOpenChange(false);
    }
  }, [locked, open, onOpenChange]);

  useEffect(() => {
    if (!open) {
      setQrDataUrl(null);
      setPassword(null);
      setRevealed(false);
      setLoading(false);
      setError(null);
    }
  }, [open, detail.id]);

  useEffect(() => {
    if (!open || locked || !wifi.ssid) return;

    let cancelled = false;
    setLoading(true);
    setError(null);

    async function loadAndGenerate() {
      try {
        let pw = '';
        if (wifi.passwordField) {
          if (wifi.passwordField.value) {
            pw = wifi.passwordField.value;
          } else if (wifi.passwordField.kind === 'concealed') {
            const revealedPw = await api.revealField({
              vaultId: detail.vaultId,
              itemId: detail.id,
              fieldId: wifi.passwordField.id,
            });
            pw = revealedPw;
          }
        }

        if (cancelled) return;
        setPassword(pw);

        const payload = buildWifiPayload({
          ssid: wifi.ssid,
          password: pw,
          security: wifi.security,
          hidden: wifi.hidden,
        });

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
          setError(errText(e) || t('wifi.errorGenerating'));
          setLoading(false);
        }
      }
    }

    void loadAndGenerate();

    return () => {
      cancelled = true;
    };
  }, [open, locked, detail.id, detail.vaultId, wifi.ssid, wifi.security, wifi.hidden, wifi.passwordField?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const isOpenNetwork = !wifi.passwordField || wifi.security.trim().toLowerCase() === 'nopass' || wifi.security.trim().toLowerCase() === 'none' || wifi.security.trim().toLowerCase() === 'open';
  const hasPassword = Boolean(password && password.length > 0);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('wifi.qrDialogTitle')}
        description={t('wifi.qrHint')}
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
            data-testid="wifi-qr-container"
          >
            {qrDataUrl ? (
              <img
                src={qrDataUrl}
                alt={t('wifi.qrCodeAlt', { ssid: wifi.ssid })}
                className="h-[220px] w-[220px] block"
                data-testid="wifi-qr-image"
              />
            ) : loading ? (
              <div className="flex h-[220px] w-[220px] items-center justify-center text-neutral-400">
                <Loader2 className="h-8 w-8 animate-spin" />
              </div>
            ) : (
              <div className="flex h-[220px] w-[220px] items-center justify-center p-4 text-xs text-neutral-500">
                {error || t('wifi.errorGenerating')}
              </div>
            )}
          </div>

          {/* Network name under the code */}
          <div
            className="mt-4 flex max-w-full items-center justify-center gap-1.5 text-lg font-semibold text-fg"
            data-testid="wifi-network-name"
          >
            <Wifi size={18} className="shrink-0 text-accent" aria-hidden="true" />
            <span className="truncate">{wifi.ssid}</span>
          </div>

          {/* "Anyone who scans this can join the network" hint */}
          <div className="mt-1 text-xs text-muted" data-testid="wifi-qr-hint">
            {t('wifi.qrHint')}
          </div>

          {/* Password row with Reveal button */}
          {hasPassword && !isOpenNetwork ? (
            <div
              className="mt-4 flex w-full items-center justify-between rounded-lg border border-border bg-card px-3.5 py-2 text-sm"
              data-testid="wifi-password-row"
            >
              <div className="min-w-0 flex-1 text-left">
                <div className="text-[11px] font-medium text-label">{t('wifi.password')}</div>
                <div
                  className={cn('font-mono text-sm tracking-wider truncate', !revealed && 'tracking-widest')}
                  data-testid="wifi-password-value"
                >
                  {revealed ? password : '••••••••'}
                </div>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="iconSm"
                aria-label={revealed ? t('wifi.concealPassword') : t('wifi.revealPassword')}
                data-testid="wifi-reveal-btn"
                onClick={() => setRevealed(!revealed)}
              >
                {revealed ? <EyeOff size={14} /> : <Eye size={14} />}
              </Button>
            </div>
          ) : isOpenNetwork ? (
            <div
              className="mt-4 rounded-md border border-border/70 bg-card/60 px-3 py-1.5 text-xs text-muted"
              data-testid="wifi-open-badge"
            >
              {t('wifi.openNetwork')}
            </div>
          ) : null}

          {/* Orange accent primary action button */}
          <div className="mt-5 flex w-full justify-end">
            <Button
              type="button"
              variant="default"
              onClick={() => onOpenChange(false)}
              data-testid="wifi-qr-close-btn"
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
