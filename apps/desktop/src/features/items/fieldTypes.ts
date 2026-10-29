import { AlignLeft, Calendar, CalendarDays, KeyRound, Link, Mail, Phone, ShieldCheck, Type, type LucideIcon } from 'lucide-react';
import type { FieldKind } from '../../api/types';
import type { Key } from '../../i18n';

/** Field types offered by the editor's "Add field" menu, in menu order. `label` doubles as the new field's default label. */
export const FIELD_TYPES: { kind: FieldKind; icon: LucideIcon; label: Key }[] = [
  { kind: 'text', icon: Type, label: 'editor.type.text' },
  { kind: 'concealed', icon: KeyRound, label: 'editor.type.concealed' },
  { kind: 'url', icon: Link, label: 'editor.type.url' },
  { kind: 'email', icon: Mail, label: 'editor.type.email' },
  { kind: 'phone', icon: Phone, label: 'editor.type.phone' },
  { kind: 'date', icon: Calendar, label: 'editor.type.date' },
  { kind: 'month_year', icon: CalendarDays, label: 'editor.type.month_year' },
  { kind: 'totp', icon: ShieldCheck, label: 'editor.type.totp' },
  { kind: 'multiline', icon: AlignLeft, label: 'editor.type.multiline' },
];
