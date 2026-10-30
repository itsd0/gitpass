import { api } from '../api/client';
import type { Field, ItemCategory } from '../api/types';
import { translate, useI18n, type Key } from '../i18n';
import { toast } from '../ui';
import { useSettings } from '../features/settings/useSettings';
import { useApp } from './store';

const CARD_NUMBER_EXCLUDES = ['cvv', 'cvc', 'verification', 'security', 'pin', 'holder', 'name', 'type', 'expir', 'valid'];
const CVV_MARKERS = ['cvv', 'cvc', 'verification', 'security code'];

function hasMarker(label: string, keys: string[]) {
  return keys.some((k) => label.includes(k));
}

/** Mirrors gui-core `is_secret_copy`: concealed/TOTP, plus legacy plain-text card and bank secrets. */
export function isConcealedCopy(field: Pick<Field, 'id' | 'label' | 'kind'>, category?: ItemCategory | string): boolean {
  if (field.id.startsWith('password_history:')) return true;
  if (field.kind === 'concealed' || field.kind === 'totp') return true;
  const l = field.label.toLowerCase();
  if (category === 'credit_card') {
    const excluded = field.id === 'cvv' || field.id === 'pin' || hasMarker(l, CARD_NUMBER_EXCLUDES);
    const number = !excluded && (field.id === 'number' || l.includes('number') || l.includes('card'));
    const cvv = field.id === 'cvv' || field.id === 'cvc' || hasMarker(l, CVV_MARKERS);
    if (number || cvv) return true;
  }
  if (category === 'bank_account' && (field.id === 'account_number' || field.id === 'pin' || l.includes('account number') || l.includes('pin'))) {
    return true;
  }
  return false;
}

export function concealedCopyTitle(field: Pick<Field, 'id' | 'label' | 'kind'>): string {
  const tr = (k: Key) => translate(useI18n.getState().lang, k);
  if (field.kind === 'totp' || /one-time|totp|\botp\b/i.test(field.label)) return tr('itemKeys.totpCopied');
  if (/pass/i.test(field.label) || field.id.startsWith('password_history:')) return tr('itemKeys.passwordCopied');
  return tr('itemKeys.secretCopied');
}

async function clipboardClearSeconds(): Promise<number> {
  const cur = useSettings.getState().settings?.clipboardClearSecs;
  if (cur != null) return cur;
  try {
    return (await api.getSettings()).clipboardClearSecs;
  } catch {
    return 30;
  }
}

function pushClipboardToast(base: string, seconds: number) {
  toast(base, {
    duration: seconds > 0 ? seconds * 1000 + 4000 : 8000,
    clipboard: {
      base,
      seconds,
      onClearNow: () => api.clearClipboardIfOurs(),
      onOpenSetting: () => {
        useApp.getState().setRoute({ name: 'settings', tab: 'security', highlight: 'setting-clipboard' });
      },
    },
  });
}

/** Countdown toast after a concealed copy. Callers keep a plain toast for usernames, URLs and other non-secrets. */
export function toastConcealedCopied(base: string) {
  const known = useSettings.getState().settings?.clipboardClearSecs;
  if (known != null) {
    pushClipboardToast(base, known);
    return;
  }
  void clipboardClearSeconds().then((seconds) => pushClipboardToast(base, seconds));
}

export function toastFieldCopied(field: Pick<Field, 'id' | 'label' | 'kind'>, category?: ItemCategory | string) {
  if (isConcealedCopy(field, category)) toastConcealedCopied(concealedCopyTitle(field));
  else toast(`Copied ${field.label}`);
}
