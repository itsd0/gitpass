import { ShieldCheck } from 'lucide-react';
import type { FeatureDef } from '../registry';
import { useApp } from '../../lib/store';
import { WatchtowerPage } from './WatchtowerPage';
export const feature: FeatureDef = { id: 'watchtower', route: 'watchtower', sidebar: { label: 'nav.watchtower', icon: ShieldCheck, section: 'top' }, Page: WatchtowerPage,
  commands: [{ id: 'go-watchtower', label: 'Open Watchtower', labelKey: 'cmd.watchtower', run: () => useApp.getState().setRoute({ name: 'watchtower' }) }] };
