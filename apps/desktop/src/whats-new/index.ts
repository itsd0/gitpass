import { Sparkles } from 'lucide-react';
import type { FeatureDef } from '../features/registry';
import { useApp } from '../lib/store';
import { WhatsNewModal } from './WhatsNewModal';

export * from './types';
export * from './semver';
export * from './releases';
export * from './WhatsNewModal';
export * from './useWhatsNew';

export const whatsNewFeature: FeatureDef = {
  id: 'whats-new',
  Modal: WhatsNewModal,
  commands: [
    {
      id: 'whats-new',
      label: "What's new",
      labelKey: 'cmd.whatsNew',
      icon: Sparkles,
      keywords: ['what', 'new', 'release', 'notes', 'changelog', 'update'],
      run: () => useApp.getState().openModal('whats-new'),
    },
  ],
};
