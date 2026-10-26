export { QuickAccess, QuickAccessPage, pickFields } from './QuickAccess';
export const isQuickAccessWindow = () => typeof window !== 'undefined' && (window.location.hash === '#/quick-access' || new URLSearchParams(window.location.search).get('window') === 'quick-access');
import { Zap } from 'lucide-react';
import { useApp } from '../../lib/store';
import { api } from '../../api/client';
import { QuickAccessRoute } from './QuickAccess';
import type { FeatureDef } from '../registry';
/** Separate Tauri window when wired (see README); in the main window/browser it is also a route. */
export const feature: FeatureDef = { id: 'quick-access', route: 'quick-access', Page: QuickAccessRoute,
  commands: [{ id: 'quick-access', label: 'Open Quick Access', labelKey: 'qa.label', icon: Zap, keywords: ['quick', 'fill', 'copy'], run: () => { if ((window as any).__TAURI_INTERNALS__) void api.showQuickAccess(); else useApp.getState().setRoute({ name: 'quick-access' }); } }] };
