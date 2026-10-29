import { api } from '../../api/client';
import type { Field, ItemDetail } from '../../api/types';
import { toast } from '../../ui';
import { translate, useI18n, type Key } from '../../i18n';
import { pickFields } from '../quick-access/QuickAccess';

export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

const tr = (k: Key) => translate(useI18n.getState().lang, k);

/** The field a plain "copy" takes: password (logins, Wi-Fi), card number, else the first concealed field (API credential). */
export function pickPrimarySecret(d: ItemDetail): Field | undefined {
  const fields = d.sections.flatMap((s) => s.fields).filter((x) => x.kind === 'concealed');
  return fields.find((x) => /pass/i.test(x.label)) ?? fields.find((x) => /^(card )?number$/i.test(x.label)) ?? fields[0];
}

/** Copy the primary secret of an item. The copy happens in Rust (concealed + transient pasteboard); the value never reaches JS. */
export async function copyPrimarySecret(vaultId: string, itemId: string) {
  try {
    const f = pickPrimarySecret(await api.getItem({ vaultId, itemId }));
    if (!f) { toast(tr('itemKeys.noSecret')); return; }
    await api.copyField({ vaultId, itemId, fieldId: f.id });
    toast(tr(/pass/i.test(f.label) ? 'itemKeys.passwordCopied' : 'itemKeys.secretCopied'));
  } catch (e) { toast(errText(e), 'danger'); }
}

/** Copy the username (or email) of an item. */
export async function copyUsername(vaultId: string, itemId: string) {
  try {
    const f = pickFields(await api.getItem({ vaultId, itemId })).username;
    if (!f) { toast(tr('itemKeys.noUsername')); return; }
    await api.copyField({ vaultId, itemId, fieldId: f.id });
    toast(tr('itemKeys.usernameCopied'));
  } catch (e) { toast(errText(e), 'danger'); }
}

/** Copy the current one-time code of an item. */
export async function copyTotpCode(vaultId: string, itemId: string) {
  try {
    const f = pickFields(await api.getItem({ vaultId, itemId })).totp;
    if (!f) { toast(tr('itemKeys.noTotp')); return; }
    await api.copyField({ vaultId, itemId, fieldId: f.id });
    toast(tr('itemKeys.totpCopied'));
  } catch (e) { toast(errText(e), 'danger'); }
}

/** Open the item's first http(s) website in the default browser, the same way the detail view's Open link does. */
export async function openItemWebsite(vaultId: string, itemId: string) {
  try {
    const d = await api.getItem({ vaultId, itemId });
    const url = [d.url, ...d.sections.flatMap((s) => s.fields).filter((x) => x.kind === 'url').map((x) => x.value)]
      .find((u): u is string => !!u && /^https?:\/\//i.test(u));
    if (!url) { toast(tr('itemKeys.noWebsite')); return; }
    const a = document.createElement('a');
    a.href = url; a.target = '_blank'; a.rel = 'noreferrer noopener';
    a.click();
  } catch (e) { toast(errText(e), 'danger'); }
}
