import type { FeatureDef } from '../registry';
import { useApp } from '../../lib/store';
import { NewItemModal, NewItemModalHost } from './NewItemModal';
export { NewItemModal };
export const feature: FeatureDef = {
  id: 'new-item',
  Modal: NewItemModalHost,
  commands: [
    { id: 'new-item', label: 'New item', labelKey: 'cmd.newItem', shortcut: 'mod+n', keywords: ['add', 'create'], run: () => useApp.getState().openModal('new-item') },
    { id: 'import-data', label: 'Import data', keywords: ['migrate', 'bitwarden', 'csv'], run: () => useApp.getState().openModal('new-item', { view: 'import' }) },
  ],
};
