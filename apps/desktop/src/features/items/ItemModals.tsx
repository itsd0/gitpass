import { useState } from 'react';
import { Tag } from 'lucide-react';
import { useApp } from '../../lib/store';
import { useVaultsAndTags } from '../../lib/data';
import { useItemsUI } from './state';
import { readSel, selectedIds } from './selection';
import { runBulk } from './BulkPane';
import { useT } from '../../i18n';
import { Button, Dialog, DialogContent, Input } from '../../ui';
import { VaultIcon } from '../home/vaultIcons';
import type { FeatureDef } from '../registry';

export function MoveToVaultModal() {
  const { modal, closeModal } = useApp();
  const { vaults } = useVaultsAndTags();
  const allItems = useItemsUI((s) => s.items);
  const t = useT();

  if (modal?.id !== 'move-to-vault') return null;

  const onPick = (vaultId: string) => {
    const sIds = selectedIds(readSel());
    const selectedItems = allItems.filter((x) => sIds.includes(x.id));
    closeModal();
    void runBulk('move', { toVaultId: vaultId }, selectedItems);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && closeModal()}>
      <DialogContent title={t('moveToVault.title')} description={t('moveToVault.select')}>
        <div className="mt-3 max-h-72 space-y-1 overflow-y-auto" data-testid="move-to-vault-list">
          {vaults.map((v) => (
            <button
              key={v.id}
              onClick={() => onPick(v.id)}
              className="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm hover:bg-hover"
              data-testid={`move-vault-option-${v.id}`}
            >
              <VaultIcon icon={v.icon} color={v.color} size={18} />
              <span className="flex-1 font-medium">{v.name}</span>
              <span className="text-xs text-muted">{v.itemCount}</span>
            </button>
          ))}
        </div>
        <div className="mt-4 flex justify-end">
          <Button variant="outline" onClick={closeModal}>
            {t('common.cancel')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function AddTagModal() {
  const { modal, closeModal } = useApp();
  const { tags } = useVaultsAndTags();
  const allItems = useItemsUI((s) => s.items);
  const t = useT();
  const [tag, setTag] = useState('');

  if (modal?.id !== 'add-tag') return null;

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = tag.trim();
    if (!trimmed) return;
    const sIds = selectedIds(readSel());
    const selectedItems = allItems.filter((x) => sIds.includes(x.id));
    closeModal();
    void runBulk('add_tag', { tag: trimmed }, selectedItems);
  };

  return (
    <Dialog open onOpenChange={(o) => !o && closeModal()}>
      <DialogContent title={t('addTag.title')}>
        <form onSubmit={onSubmit} className="mt-3 space-y-4">
          <div>
            <Input
              autoFocus
              list="palette-add-tag-suggestions"
              value={tag}
              onChange={(e) => setTag(e.target.value)}
              placeholder={t('addTag.placeholder')}
              aria-label={t('addTag.title')}
              data-testid="add-tag-modal-input"
            />
            <datalist id="palette-add-tag-suggestions">
              {tags.map((tName) => (
                <option key={tName} value={tName} />
              ))}
            </datalist>
          </div>
          <div className="flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={closeModal}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" disabled={!tag.trim()} data-testid="add-tag-modal-submit">
              <Tag size={14} className="mr-1.5" />
              {t('bulk.add')}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export const moveToVaultFeature: FeatureDef = {
  id: 'move-to-vault',
  Modal: MoveToVaultModal,
};

export const addTagFeature: FeatureDef = {
  id: 'add-tag',
  Modal: AddTagModal,
};
