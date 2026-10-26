export type CardBrand = 'visa' | 'mastercard' | 'amex' | 'discover' | 'diners' | 'jcb' | 'unionpay' | 'unknown';
export function detectBrand(raw: string): CardBrand {
  const n = raw.replace(/\D/g, '');
  if (/^4/.test(n)) return 'visa';
  if (/^3[47]/.test(n)) return 'amex';
  if (/^(5[1-5]|2(2[2-9][1-9]|2[3-9]\d|[3-6]\d\d|7[01]\d|720))/.test(n)) return 'mastercard';
  if (/^(6011|65|64[4-9])/.test(n)) return 'discover';
  if (/^3(0[0-5]|[689])/.test(n)) return 'diners';
  if (/^35(2[89]|[3-8])/.test(n)) return 'jcb';
  if (/^62/.test(n)) return 'unionpay';
  return 'unknown';
}
export const brandLabel: Record<CardBrand, string> = { visa: 'Visa', mastercard: 'Mastercard', amex: 'American Express', discover: 'Discover', diners: 'Diners Club', jcb: 'JCB', unionpay: 'UnionPay', unknown: '' };
export function maxLength(b: CardBrand) { return b === 'amex' ? 15 : b === 'diners' ? 14 : 19; }
export function formatCardNumber(raw: string): string {
  const b = detectBrand(raw);
  const n = raw.replace(/\D/g, '').slice(0, maxLength(b));
  const groups = b === 'amex' ? [4, 6, 5] : b === 'diners' ? [4, 6, 4] : [4, 4, 4, 4, 3];
  const out: string[] = []; let i = 0;
  for (const g of groups) { if (i >= n.length) break; out.push(n.slice(i, i + g)); i += g; }
  return out.join(' ');
}
export function luhnValid(raw: string): boolean {
  const n = raw.replace(/\D/g, ''); if (n.length < 12) return false;
  let sum = 0; for (let i = 0; i < n.length; i++) { let d = +n[n.length - 1 - i]; if (i % 2) { d *= 2; if (d > 9) d -= 9; } sum += d; }
  return sum % 10 === 0;
}
/** "MM/YYYY" for a month_year field */
export function formatExpiry(month: string, year: string) { return month && year ? `${month.padStart(2, '0')}/${year}` : ''; }
