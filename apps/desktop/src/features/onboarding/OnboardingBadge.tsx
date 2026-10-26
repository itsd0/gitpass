import { Badge } from '../../ui';
import { useOnboarding } from './useOnboarding';
import { stepsLeft } from './steps';
/** Sidebar badge: "N steps left". Renders nothing when done or dismissed. */
export function OnboardingBadge() {
  const { onboarding } = useOnboarding(); const n = stepsLeft(onboarding);
  if (!onboarding || onboarding.dismissed || n === 0) return null;
  return <Badge>{n} {n === 1 ? 'step' : 'steps'} left</Badge>;
}
