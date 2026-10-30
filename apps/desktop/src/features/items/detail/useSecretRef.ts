import { useMemo } from 'react';
import type { ItemDetail as Detail, VaultInfo } from '../../../api/types';
import { useItemsUI } from '../state';
import { useT } from '../../../i18n';
import { toast } from '../../../ui';
import { refTarget as buildRefTarget, secretRef } from '../../../lib/secretRef';

export function useSecretRef(detail: Detail, vault?: VaultInfo, vaults: VaultInfo[] = []) {
  const t = useT();
  const siblings = useItemsUI((s) => s.items);
  const refTarget = useMemo(
    () => buildRefTarget(vault, vaults, detail, siblings.filter((i) => i.vaultId === detail.vaultId), detail.sections.flatMap((s) => s.fields)),
    [vault, vaults, detail, siblings],
  );
  const copyPasswordRef = () => {
    const f = detail.sections.flatMap((s) => s.fields).find((x) => x.id === 'password');
    if (!refTarget || !f || !navigator.clipboard) { toast(t('ref.failed'), 'danger'); return; }
    navigator.clipboard.writeText(secretRef(refTarget, f)).then(() => toast(t('ref.copied')), () => toast(t('ref.failed'), 'danger'));
  };

  return { refTarget, copyPasswordRef };
}
