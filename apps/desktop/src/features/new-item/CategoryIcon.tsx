import { KeyRound, StickyNote, CreditCard, IdCard, Code2, FileText, Terminal, Wifi } from 'lucide-react';
import type { ItemCategory } from '../../api/types';
import { categoryById } from './categories';
const ICONS = { login: KeyRound, secure_note: StickyNote, credit_card: CreditCard, identity: IdCard, api_credential: Code2, document: FileText, ssh_key: Terminal, wifi: Wifi } as const;
export function CategoryIcon({ category, size = 36 }: { category: ItemCategory; size?: number }) {
  const Icon = ICONS[category];
  return <span className={`inline-flex items-center justify-center rounded-lg bg-gradient-to-br ${categoryById(category).color} text-white shadow`} style={{ width: size, height: size }}><Icon size={size * 0.55} /></span>;
}
