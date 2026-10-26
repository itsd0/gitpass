import { Home } from 'lucide-react';
import type { FeatureDef } from '../registry';
import { useApp } from '../../lib/store';
import { HomePage } from './HomePage';
export const feature: FeatureDef = { id: 'home', route: 'home', sidebar: { label: 'nav.home', icon: Home, section: 'top' }, Page: HomePage,
  commands: [{ id: 'go-home', label: 'Go to Home', labelKey: 'cmd.home', run: () => useApp.getState().setRoute({ name: 'home' }) }] };
