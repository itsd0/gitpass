import { useApp } from '../../lib/store';
import type { Shortcut } from '../palette/shortcuts';
import { copyPrimarySecret, copyTotpCode, copyUsername, openItemWebsite } from './actions';
import { useItemsUI } from './state';

/** The single selected item that keyboard shortcuts act on, or null (nothing selected, multi-select, other page, dialog or editor open). */
export function shortcutTarget(opts: { ignoreModal?: boolean } = {}) {
  const a = useApp.getState();
  if (a.route.name !== 'items' || (a.modal && !opts.ignoreModal) || a.editing || a.multiIds.length > 0 || !a.selectedItemId) return null;
  const it = useItemsUI.getState().items.find((x) => x.id === a.selectedItemId);
  return it ? { vaultId: it.vaultId, itemId: it.id, deleted: !!it.deletedAt } : null;
}

const hasTextSelection = () => !!window.getSelection()?.toString();
const onItem = (fn: (vaultId: string, itemId: string) => unknown) => () => { const t = shortcutTarget(); if (t) void fn(t.vaultId, t.itemId); };
export const editSelectedItem = () => { const t = shortcutTarget(); if (t && !t.deleted) useApp.getState().setEditing(true); };

export const copyPasswordSelected = onItem(copyPrimarySecret);
export const copyUsernameSelected = onItem(copyUsername);
export const copyTotpSelected = onItem(copyTotpCode);
export const openWebsiteSelected = onItem(openItemWebsite);

/**
 * Shortcuts for the selected item. They sit behind `when` so ⌘C stays a native copy while text is selected or an
 * input has focus (the global handler also skips inputs), and they do nothing in multi-select or without a selection.
 */
export const itemShortcuts: Shortcut[] = [
  { id: 'item-copy-password', combo: 'mod+c', labelKey: 'itemKeys.copyPassword', run: copyPasswordSelected, when: () => !!shortcutTarget() && !hasTextSelection() },
  { id: 'item-copy-username', combo: 'mod+shift+c', labelKey: 'itemKeys.copyUsername', run: copyUsernameSelected, when: () => !!shortcutTarget() },
  { id: 'item-copy-totp', combo: 'mod+alt+c', labelKey: 'itemKeys.copyTotp', run: copyTotpSelected, when: () => !!shortcutTarget() },
  { id: 'item-edit', combo: 'mod+e', labelKey: 'itemKeys.edit', run: editSelectedItem, when: () => !!shortcutTarget() },
  { id: 'item-open-website', combo: 'mod+shift+o', labelKey: 'itemKeys.openWebsite', run: openWebsiteSelected, when: () => !!shortcutTarget() },
];
