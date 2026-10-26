import { CreditCard, FileText, Fingerprint, KeyRound, Code2, StickyNote, UserSquare, Wifi, Terminal } from 'lucide-react';
import type { ItemCategory, ItemSummary } from '../../api/types';
import { Avatar } from '../../ui';

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

function cardBrand(subtitle: string): string {
  const d = subtitle.replace(/\D/g, '')[0];
  return d === '4' ? 'VISA' : d === '5' ? 'MC' : d === '3' ? 'AMEX' : 'CARD';
}

export function ItemIcon({ item, size = 28 }: { item: Pick<ItemSummary, 'category' | 'title' | 'subtitle'>; size?: number }) {
  if (item.category === 'credit_card') {
    const b = cardBrand(item.subtitle);
    return (
      <span className="inline-flex shrink-0 items-center justify-center rounded-md bg-[#1a3fc4] font-extrabold italic tracking-tight text-white"
        style={{ width: size * 1.15, height: size, fontSize: size * (b.length > 3 ? 0.3 : 0.36) }} data-testid="card-badge">{b}</span>
    );
  }
  if (item.category === 'login') return <Avatar name={item.title} size={size} />;
  const m = catMeta(item.category); const I = m.icon;
  return <span className="inline-flex shrink-0 items-center justify-center rounded-md text-white" style={{ width: size, height: size, background: m.color }}><I size={size * 0.58} /></span>;
}
