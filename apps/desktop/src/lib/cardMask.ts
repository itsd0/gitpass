import { brandLabel, detectBrand } from '../features/new-item/card';
/** Display helpers for card numbers. Only ever used on already-decrypted values in memory; nothing here is persisted. */
export const cardDigits = (v: string | null | undefined) => (v ?? '').replace(/\D/g, '');
export function cardLabel(v: string): string {
  const d = cardDigits(v); if (d.length < 8) return '';
  const b = brandLabel[detectBrand(d)]; return `${b ? b + ' ' : ''}•••• ${d.slice(-4)}`;
}
export function cardMask(v: string): string { const d = cardDigits(v); return d.length < 8 ? '' : `•••• •••• •••• ${d.slice(-4)}`; }
/** Full number grouped in 4s (reveal). */
export const groupCardNumber = (v: string) => cardDigits(v).replace(/(.{4})(?=.)/g, '$1 ');

export const BULLETS = '••••••••';
export function secretHint(s: string): string {
  const chars = Array.from(s);
  const n = chars.length;
  if (n >= 20) return chars.slice(0, 4).join('') + BULLETS + chars.slice(-4).join('');
  if (n >= 12) return chars.slice(0, 4).join('') + BULLETS;
  if (n >= 6) return chars.slice(0, 2).join('') + BULLETS;
  return BULLETS;
}
