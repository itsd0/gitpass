import { builtinCommands } from './commands';
export { CommandPalette, PaletteHost, ShortcutsHelp } from './CommandPalette';
export { registerShortcut, useShortcut, listShortcuts } from './shortcuts';
export { fuzzyScore, rank } from './fuzzy';
export type { PaletteCommand } from './commands';
/** PaletteHost is mounted permanently by AppShell (it owns Cmd/Ctrl+K and reacts to openModal('palette') /
 * openModal('shortcuts-help')), so no registry Modal here; builtins are exposed as registry commands. */
export const feature = { id: 'palette', get commands() { return builtinCommands(); } };
