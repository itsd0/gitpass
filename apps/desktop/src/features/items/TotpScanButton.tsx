import { useState } from 'react';
import { Clipboard, Monitor, QrCode } from 'lucide-react';
import { api } from '../../api/client';
import type { ScannedTotp } from '../../api/types';
import { useT } from '../../i18n';
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, toast } from '../../ui';

export function handleScanError(e: unknown, t: (key: any, vars?: any) => string) {
  const msg = String((e as Error)?.message ?? e);
  if (/screen capture was cancelled/i.test(msg)) {
    toast(t('totp.captureCancelled'), 'danger');
  } else if (/no qr code found/i.test(msg)) {
    toast(t('totp.noQrFound'), 'danger');
  } else if (/not a one-time code/i.test(msg) || /isn't a one-time code/i.test(msg)) {
    toast(t('totp.notTotp'), 'danger');
  } else {
    toast(msg, 'danger');
  }
}

export async function handleTotpImageDrop(
  e: React.DragEvent,
  onScan: (scanned: ScannedTotp) => void,
  t: (key: any, vars?: any) => string
) {
  e.preventDefault();
  e.stopPropagation();
  const file = e.dataTransfer.files?.[0];
  if (!file) return;
  if (!file.type.startsWith('image/') && !file.name.match(/\.(png|jpe?g|webp|gif|bmp)$/i)) {
    toast(t('totp.notTotp'), 'danger');
    return;
  }
  const reader = new FileReader();
  reader.onload = async () => {
    try {
      const dataBase64 = reader.result as string;
      const res = await api.scanTotp({ source: 'image', dataBase64 });
      onScan(res);
    } catch (err) {
      handleScanError(err, t);
    }
  };
  reader.readAsDataURL(file);
}

export function TotpScanButton({
  onScan,
  compact = false,
}: {
  onScan: (scanned: ScannedTotp) => void;
  compact?: boolean;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);

  const runScan = async (source: 'screen' | 'clipboard') => {
    setBusy(true);
    try {
      const res = await api.scanTotp({ source });
      onScan(res);
    } catch (err) {
      handleScanError(err, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size={compact ? 'iconSm' : 'sm'}
          aria-label={t('totp.scanQr')}
          title={t('totp.scanQr')}
          disabled={busy}
          className="shrink-0 gap-1.5 border-border/80 text-xs font-medium text-fg transition-colors hover:border-accent hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-1"
          data-testid="totp-scan-button"
        >
          <QrCode size={compact ? 13 : 14} className="shrink-0" />
          {!compact && <span>{t('totp.scanQr')}</span>}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuItem
          onSelect={() => runScan('screen')}
          data-testid="scan-qr-screen"
          className="gap-2 text-xs"
        >
          <Monitor size={14} className="shrink-0" />
          <span>{t('totp.scanScreen')}</span>
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => runScan('clipboard')}
          data-testid="scan-qr-clipboard"
          className="gap-2 text-xs"
        >
          <Clipboard size={14} className="shrink-0" />
          <span>{t('totp.scanClipboard')}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function TotpAutofillOffer({
  suggestion,
  onApply,
  onDismiss,
}: {
  suggestion: { title?: string | null; username?: string | null } | null;
  onApply: (s: { title?: string; username?: string }) => void;
  onDismiss: () => void;
}) {
  const t = useT();
  if (!suggestion) return null;
  const hasTitle = Boolean(suggestion.title?.trim());
  const hasUser = Boolean(suggestion.username?.trim());
  if (!hasTitle && !hasUser) return null;

  let msg = '';
  if (hasTitle && hasUser) {
    msg = t('totp.autofillOffer', { title: suggestion.title!, username: suggestion.username! });
  } else if (hasTitle) {
    msg = t('totp.autofillTitleOnly', { title: suggestion.title! });
  } else {
    msg = t('totp.autofillUsernameOnly', { username: suggestion.username! });
  }

  return (
    <div
      data-testid="totp-autofill-suggestion"
      className="mt-2 flex items-center justify-between gap-2 rounded-md border border-accent/40 bg-accent/10 px-3 py-2 text-xs animate-in fade-in"
    >
      <span className="text-fg">{msg}</span>
      <div className="flex shrink-0 gap-1.5">
        <Button
          type="button"
          size="sm"
          data-testid="totp-autofill-apply"
          className="h-6 bg-accent px-2 text-xs text-accent-fg hover:bg-accent/90"
          onClick={() =>
            onApply({
              title: suggestion.title ?? undefined,
              username: suggestion.username ?? undefined,
            })
          }
        >
          {t('totp.autofillApply')}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          data-testid="totp-autofill-dismiss"
          className="h-6 px-2 text-xs text-muted hover:text-fg"
          onClick={onDismiss}
        >
          {t('totp.autofillDismiss')}
        </Button>
      </div>
    </div>
  );
}
