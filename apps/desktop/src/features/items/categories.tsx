import { CreditCard, FileText, Fingerprint, KeyRound, Code2, StickyNote, UserSquare, Wifi, Terminal } from 'lucide-react';
import type { ItemCategory, ItemSummary } from '../../api/types';
import { CategoryIcon } from '../../components/CategoryIcon';

export const CATEGORIES: { id: ItemCategory; label: string; icon: typeof KeyRound; color: string }[] = [
  { id: 'login', label: 'Login', icon: KeyRound, color: '#3b82f6' },
  { id: 'credit_card', label: 'Credit Card', icon: CreditCard, color: '#1a3fc4' },
  { id: 'secure_note', label: 'Secure Note', icon: StickyNote, color: '#eab308' },
  { id: 'identity', label: 'Identity', icon: UserSquare, color: '#22c55e' },
  { id: 'api_credential', label: 'API Credential', icon: Code2, color: '#14b8a6' },
  { id: 'document', label: 'Document', icon: FileText, color: '#64748b' },
  { id: 'ssh_key', label: 'SSH Key', icon: Terminal, color: '#a855f7' },
  { id: 'wifi', label: 'Wireless Router', icon: Wifi, color: '#0ea5e9' },
];
export const catMeta = (c: ItemCategory) => CATEGORIES.find((x) => x.id === c) ?? { id: c, label: c, icon: Fingerprint, color: '#64748b' };

export function ItemIcon({ item, size = 20 }: { item: Pick<ItemSummary, 'category' | 'title' | 'subtitle'>; size?: number }) {
  return <CategoryIcon category={item.category} size={size} title={catMeta(item.category).label} />;
}
