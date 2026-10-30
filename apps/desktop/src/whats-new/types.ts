import type { LucideIcon } from 'lucide-react';
import type { Route } from '../lib/store';
import type { Lang } from '../i18n';

export interface LocalizedText {
  en: string;
  my?: string;
  th?: string;
}

export interface WhatsNewShowMe {
  label?: LocalizedText;
  route?: Route;
  action?: () => void;
}

export interface WhatsNewHighlight {
  id: string;
  icon?: LucideIcon;
  title: LocalizedText;
  description: LocalizedText;
  showMe?: WhatsNewShowMe;
}

export interface ReleaseHighlights {
  version: string;
  highlights: WhatsNewHighlight[];
}

export function getLocalizedText(text: LocalizedText, lang: Lang): string {
  return text[lang] || text.en;
}
