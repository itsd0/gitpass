import { Settings as Icon } from 'lucide-react';
import { SettingsPage } from './SettingsPage';
export { useSettings } from './useSettings';
export { applyTheme } from './theme';
export const feature = { id: 'settings', route: 'settings', sidebar: { label: 'nav.settings', icon: Icon, section: 'bottom' as const }, Page: SettingsPage };
