import { api } from '../../api/client';
import type { ItemDetail } from '../../api/types';
import { toast } from '../../ui';

export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Copy the primary password (or first concealed field) of an item. Copy happens in Rust. */
export async function copyPrimarySecret(vaultId: string, itemId: string) {
  try {
    const d: ItemDetail = await api.getItem({ vaultId, itemId });
    const fields = d.sections.flatMap((s) => s.fields);
    const f = fields.find((x) => x.kind === 'concealed' && /password/i.test(x.label)) ?? fields.find((x) => x.kind === 'concealed');
    if (!f) { toast('No password on this item'); return; }
    await api.copyField({ vaultId, itemId, fieldId: f.id });
    toast(`Copied ${f.label}`);
  } catch (e) { toast(errText(e), 'danger'); }
}
