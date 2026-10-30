import { ShieldCheck } from 'lucide-react';
import type { FeatureDef } from '../registry';
import { useApp } from '../../lib/store';
import { WatchtowerPage } from './WatchtowerPage';
import { SecurityCheckupModalHost } from './SecurityCheckup';
export const feature: FeatureDef = { id: 'watchtower', route: 'watchtower', sidebar: { label: 'nav.watchtower', icon: ShieldCheck, section: 'top' }, Page: WatchtowerPage,
  commands: [{ id: 'go-watchtower', label: 'Open Watchtower', labelKey: 'cmd.watchtower', run: () => useApp.getState().setRoute({ name: 'watchtower' }) }] };

/** Security checkup modal (#401): walk actionable findings one at a time. */
export const checkupFeature: FeatureDef = {
  id: 'security-checkup',
  Modal: SecurityCheckupModalHost,
  commands: [{
    id: 'start-checkup',
    label: 'Start security checkup',
    labelKey: 'cmd.startCheckup',
    keywords: ['security', 'checkup', 'audit', 'fix', 'watchtower'],
    icon: ShieldCheck,
    run: () => {
      useApp.getState().setRoute({ name: 'watchtower' });
      useApp.getState().openModal('security-checkup');
    },
  }],
};
