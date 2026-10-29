import { Copy, ExternalLink } from 'lucide-react';
import { api } from '../../api/client';
import type { ItemDetail as Detail } from '../../api/types';
import { useApp } from '../../lib/store';
import { useT } from '../../i18n';
import type { TranslateFn } from './lastUsedAge';
import { fmtDateLong } from '../../lib/utils';
import { Badge, Button, toast } from '../../ui';
import { SiteIcon } from '../../components/SiteIcon';
import { parseLastUsedDate } from './lastUsedAge';
import { useItemsUI } from './state';
import { errText } from './actions';

type Field = Detail['sections'][number]['fields'][number];

/**
 * Field keys rendered by this section. Real (Rust) items use the core ids
 * (`rp_id`, `user_name`, `credential_id`, `sign_count`); the dev mock uses
 * random ids with human labels (`site`, `user name`, `credential id`,
 * `sign count`). Matching is by id OR label, normalised, so both work.
 * The `private_key` field is deliberately never looked up here: it stays in
 * its generic concealed row, and in the real DTO secret values are stripped
 * before they reach the webview (`to_dto_sections` in gui-core).
 */
const FIELD_KEYS: Record<'site' | 'account' | 'credential' | 'signCount', string[]> = {
  site: ['rp_id', 'site', 'website'],
  account: ['user_name', 'user name', 'username'],
  credential: ['credential_id', 'credential id'],
  signCount: ['sign_count', 'sign count'],
};

function norm(s: string): string {
  return s.trim().toLowerCase().replace(/[\s_-]+/g, '');
}

function findField(detail: Detail, keys: string[]): Field | null {
  const wanted = new Set(keys.map(norm));
  for (const s of detail.sections) {
    for (const f of s.fields) {
      if (wanted.has(norm(f.id)) || wanted.has(norm(f.label))) return f;
    }
  }
  return null;
}

/** True when a generic field row is already shown by the passkey section. */
export function isPasskeyDetailField(f: { id: string; label: string }): boolean {
  const all = new Set(Object.values(FIELD_KEYS).flat().map(norm));
  return all.has(norm(f.id)) || all.has(norm(f.label));
}

/** `https://<rp_id>` host, or the host of the item's saved URL as a fallback. */
export function passkeySite(detail: Detail, siteValue: string | null): string | null {
  const fromValue = siteValue?.trim().toLowerCase();
  if (fromValue) return fromValue;
  const url = detail.url?.trim();
  if (!url) return null;
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return null;
  }
}

function hostOf(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
  } catch {
    const host = url.replace(/^https?:\/\//i, '').split(/[\/?#]/)[0].toLowerCase().replace(/^www\./, '');
    return host || null;
  }
}

/** Shorten a credential id to first 8 … last 8, monospace display. */
export function shortenCredentialId(value: string): string {
  return value.length > 20 ? `${value.slice(0, 8)}…${value.slice(-8)}` : value;
}

/** Relative age ("Today", "3 days ago", …) using the passkey.detail.age* keys. */
function relativeAge(input: string | number | null | undefined, now: Date, t: TranslateFn): string | null {
  const date = parseLastUsedDate(input);
  if (!date) return null;
  const diffMs = now.getTime() - date.getTime();
  if (diffMs < 0) return t('passkey.detail.ageToday');
  const diffDays = Math.floor(diffMs / 86400000);
  const monthDiff = (now.getUTCFullYear() - date.getUTCFullYear()) * 12 + (now.getUTCMonth() - date.getUTCMonth());
  if (monthDiff >= 12) {
    const years = Math.floor(monthDiff / 12);
    return years === 1 ? t('passkey.detail.ageYearOne') : t('passkey.detail.ageYears', { n: years });
  }
  if (monthDiff >= 2) return t('passkey.detail.ageMonths', { n: monthDiff });
  if (monthDiff === 1 && diffDays >= 28) return t('passkey.detail.ageMonthOne');
  if (diffDays > 1) return t('passkey.detail.ageDays', { n: diffDays });
  if (diffDays === 1) return t('passkey.detail.ageDayOne');
  return t('passkey.detail.ageToday');
}

/**
 * First-class passkey detail section, rendered under the item title.
 * Only public facts are shown here. The private key is never read or
 * rendered by this component.
 */
export function PasskeyDetail({ detail, ctx }: { detail: Detail; ctx: { vaultId: string; itemId: string } }) {
  const t = useT();
  const items = useItemsUI((s) => s.items);

  const siteField = findField(detail, FIELD_KEYS.site);
  const accountField = findField(detail, FIELD_KEYS.account);
  const credentialField = findField(detail, FIELD_KEYS.credential);
  const signCountField = findField(detail, FIELD_KEYS.signCount);

  const site = passkeySite(detail, siteField?.value ?? null);
  const account = accountField?.value || detail.subtitle || null;
  const credentialId = credentialField?.value ?? null;
  const signCount = signCountField?.value ?? null;

  if (!site && !account && !credentialId) return null;

  const normSite = site?.replace(/^www\./, '');
  const loginMatch = normSite
    ? items.find((x) => x.id !== detail.id && x.category === 'login' && hostOf(x.url) === normSite)
    : undefined;

  const created = relativeAge(detail.createdAt, new Date(), t);
  const lastUsedDate = parseLastUsedDate(detail.lastUsed);
  const lastUsed = lastUsedDate ? relativeAge(detail.lastUsed, new Date(), t) : null;

  const copyText = async (field: Field | null, value: string | null, toastMsg: string) => {
    try {
      if (field) await api.copyField({ ...ctx, fieldId: field.id });
      else if (value) await navigator.clipboard?.writeText(value);
      toast(toastMsg);
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  const openSite = async () => {
    try {
      await api.openPasskeySite({ ...ctx });
    } catch (e) {
      toast(errText(e), 'danger');
    }
  };

  return (
    <div className="mb-4 rounded-lg border border-border bg-card px-4 py-3" data-testid="passkey-detail">
      {site && (
        <div className="flex flex-wrap items-center gap-2">
          <SiteIcon item={{ title: site, url: `https://${site}` }} size={24} />
          <span className="min-w-0 flex-1 truncate font-medium" data-testid="passkey-site">{site}</span>
          <Button variant="default" size="sm" onClick={() => void openSite()} data-testid="passkey-open-site">
            <ExternalLink size={13} />{t('passkey.detail.openSite')}
          </Button>
        </div>
      )}

      {account && (
        <div className="mt-2.5 flex items-center gap-2">
          <span className="w-20 shrink-0 text-xs text-label">{t('passkey.detail.account')}</span>
          <span className="min-w-0 flex-1 truncate text-[13px]" data-testid="passkey-account">{account}</span>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={t('passkey.detail.copyAccount')}
            onClick={() => void copyText(accountField, account, t('common.copied'))}
          >
            <Copy size={13} />
          </Button>
        </div>
      )}

      {created && (
        <div className="mt-2.5 flex items-center gap-2">
          <span className="w-20 shrink-0 text-xs text-label">{t('passkey.detail.created')}</span>
          <span className="text-[13px]" title={fmtDateLong(detail.createdAt)} data-testid="passkey-created">{created}</span>
        </div>
      )}

      <div className="mt-2.5 flex items-center gap-2">
        <span className="w-20 shrink-0 text-xs text-label">{t('passkey.detail.lastUsed')}</span>
        {lastUsed && lastUsedDate ? (
          <span className="text-[13px]" title={fmtDateLong(lastUsedDate.toISOString())} data-testid="passkey-last-used">{lastUsed}</span>
        ) : (
          <span className="text-[13px] text-muted" data-testid="passkey-last-used">{t('passkey.detail.neverUsed')}</span>
        )}
      </div>

      {credentialId && (
        <div className="mt-2.5 flex items-center gap-2">
          <span className="w-20 shrink-0 text-xs text-label">{t('passkey.detail.credentialId')}</span>
          <span className="min-w-0 flex-1 truncate font-mono text-[13px]" data-testid="passkey-credential-id">{shortenCredentialId(credentialId)}</span>
          <Button
            variant="ghost"
            size="iconSm"
            aria-label={t('passkey.detail.copyCredentialId')}
            onClick={() => void copyText(credentialField, credentialId, t('common.copied'))}
          >
            <Copy size={13} />
          </Button>
        </div>
      )}

      {signCount && (
        <div className="mt-2.5 flex items-center gap-2">
          <span className="w-20 shrink-0 text-xs text-label">{t('passkey.detail.signCount')}</span>
          <span className="text-[13px]" data-testid="passkey-sign-count">{signCount}</span>
        </div>
      )}

      {loginMatch && (
        <div className="mt-2.5 flex flex-wrap items-center gap-2 text-xs text-muted" data-testid="passkey-also-login">
          <Badge tone="default">{t('passkey.detail.alsoSavedAsLogin')}</Badge>
          <Button variant="link" size="sm" onClick={() => useApp.getState().selectItem(loginMatch.id)}>
            {loginMatch.title}
          </Button>
        </div>
      )}
    </div>
  );
}
