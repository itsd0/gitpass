import type * as React from 'react';
import { feature as home } from './home';
import { feature as items } from './items';
import { feature as newItem } from './new-item';
import { feature as onboarding } from './onboarding';
import { feature as watchtower, checkupFeature } from './watchtower';
import { feature as people } from './people';
import { feature as settings } from './settings';
import { feature as palette } from './palette';
import { feature as quickAccess } from './quick-access';
import { feature as generator } from './generator';
import { feature as largeType } from './items/LargeType';
import { moveToVaultFeature, addTagFeature } from './items/ItemModals';
import { whatsNewFeature } from '../whats-new';
import { emergencyKitFeature } from './onboarding/EmergencyKitModal';

/** One palette command shape for every feature. `label` is the display text (English fallback);
 * `labelKey` (an i18n key) wins when present so the palette follows the UI language. */
export interface PaletteCommand {
  id: string; label: string; labelKey?: string; keywords?: string[];
  icon?: React.ComponentType<{ size?: number | string; className?: string }>; shortcut?: string;
  run: () => void | Promise<void>;
}
export interface FeatureDef {
  id: string;
  route?: string;
  sidebar?: { /** i18n key (e.g. 'nav.people') or literal text */ label: string; icon: React.ComponentType<{ size?: number | string; className?: string }>; section: 'top' | 'bottom' };
  Page?: React.FC;
  Modal?: React.FC;
  commands?: PaletteCommand[];
}

export const features: FeatureDef[] = [home, items, newItem, onboarding, watchtower, checkupFeature, people, settings, palette, quickAccess, generator, largeType, moveToVaultFeature, addTagFeature, whatsNewFeature, emergencyKitFeature] as FeatureDef[];
export const featureById = (id: string) => features.find((f) => f.id === id);
export const featureForRoute = (route: string) => features.find((f) => f.route === route && f.Page);
/** Modals are opened via useApp().openModal(featureId). */
export const modalFor = (id: string) => features.find((f) => f.id === id)?.Modal;
/** All feature commands, first definition of an id wins (palette builtins are listed by the palette itself). */
export const allCommands = () => { const seen = new Set<string>(); return features.flatMap((f) => f.commands ?? []).filter((c) => !seen.has(c.id) && !!seen.add(c.id)); };
