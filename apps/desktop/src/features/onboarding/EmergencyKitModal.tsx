import { KeyRound } from 'lucide-react';
import { useOnboarding } from './useOnboarding';
import { useApp } from '../../lib/store';
import type { FeatureDef } from '../registry';
import { EmergencyKitFlow } from './EmergencyKit';

/** View / save the Emergency Kit without rotating the key (Secret Key shown only after re-entering the master password). */
export function EmergencyKitModal() {
  const { modal, closeModal } = useApp(); const { patch } = useOnboarding();
  if (modal?.id !== 'emergency-kit') return null;
  const onPrinted = () => { void patch({ emergencyKitSaved: true }).catch(() => {}); closeModal(); };
  return <EmergencyKitFlow onClose={closeModal} onPrinted={onPrinted} />;
}

export const emergencyKitFeature: FeatureDef = {
  id: 'emergency-kit',
  Modal: EmergencyKitModal,
  commands: [{
    id: 'view-emergency-kit', label: 'View / Save Emergency Kit', labelKey: 'cmd.viewEmergencyKit', icon: KeyRound,
    keywords: ['emergency', 'kit', 'secret key', 'print', 'pdf', 'recovery'],
    run: () => useApp.getState().openModal('emergency-kit'),
  }],
};
