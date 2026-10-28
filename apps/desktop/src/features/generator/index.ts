import { KeyRound } from 'lucide-react';
import type { FeatureDef } from '../registry';
import { useApp } from '../../lib/store';
import { PasswordGeneratorModalHost, PasswordGenerator } from '../new-item/PasswordGenerator';

export { PasswordGenerator, PasswordGeneratorModalHost };

export const feature: FeatureDef = {
  id: 'password-generator',
  Modal: PasswordGeneratorModalHost,
  commands: [
    {
      id: 'generate-password',
      label: 'Generate password',
      labelKey: 'cmd.generatePassword',
      shortcut: 'mod+shift+g',
      keywords: ['password', 'generator', 'random', 'pin', 'words'],
      icon: KeyRound,
      run: () => useApp.getState().openModal('password-generator'),
    },
  ],
};
