import { Briefcase, Home, Lock, Shield, Star, Users, Vault, Wallet } from 'lucide-react';
export const VAULT_ICONS = { vault: Vault, lock: Lock, users: Users, briefcase: Briefcase, home: Home, star: Star, shield: Shield, wallet: Wallet } as const;
export const VAULT_COLORS = ['#64748b', '#a855f7', '#0ea5e9', '#22c55e', '#f59e0b', '#ef4444', '#ec4899', '#14b8a6'];
export function VaultIcon({ icon, color, size = 40, ring = size >= 24 }: { icon?: string | null; color?: string | null; size?: number; ring?: boolean }) {
  const I = VAULT_ICONS[(icon ?? 'vault') as keyof typeof VAULT_ICONS] ?? Vault;
  return <span className={`inline-flex shrink-0 items-center justify-center rounded-full ${ring ? 'ring-4 ring-panel' : ''}`} style={{ width: size, height: size, background: color ?? '#64748b' }}><I size={size * 0.5} color="white" /></span>;
}
