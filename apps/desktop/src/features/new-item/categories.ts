import type { FieldKind, ItemCategory, Section } from '../../api/types';
export interface FieldTpl { id: string; label: string; kind: FieldKind; }
export interface SectionTpl { id: string; title: string | null; fields: FieldTpl[]; }
export interface CategoryDef { id: ItemCategory; label: string; color: string; primary: boolean; sections: SectionTpl[]; }
const f = (id: string, label: string, kind: FieldKind = 'text'): FieldTpl => ({ id, label, kind });
export const CATEGORIES: CategoryDef[] = [
  { id: 'login', label: 'Login', color: 'from-teal-400 to-cyan-600', primary: true, sections: [{ id: 'main', title: null, fields: [f('username', 'username'), f('password', 'password', 'concealed'), f('website', 'website', 'url'), f('otp', 'one-time password', 'totp')] }] },
  { id: 'secure_note', label: 'Secure Note', color: 'from-amber-300 to-orange-500', primary: true, sections: [{ id: 'main', title: null, fields: [f('note', 'note', 'multiline')] }] },
  { id: 'credit_card', label: 'Credit Card', color: 'from-sky-300 to-blue-600', primary: true, sections: [{ id: 'main', title: null, fields: [f('cardholder', 'cardholder name'), f('type', 'type'), f('number', 'number', 'concealed'), f('cvv', 'verification number', 'concealed'), f('expiry', 'expiry date', 'month_year'), f('valid_from', 'valid from', 'month_year'), f('pin', 'PIN', 'concealed')] }] },
  { id: 'identity', label: 'Identity', color: 'from-emerald-300 to-green-600', primary: true, sections: [
    { id: 'name', title: 'Identification', fields: [f('first', 'first name'), f('last', 'last name'), f('birth', 'birth date', 'date')] },
    { id: 'address', title: 'Address', fields: [f('address', 'address', 'multiline')] },
    { id: 'contact', title: 'Contact', fields: [f('email', 'email', 'email'), f('phone', 'phone', 'phone')] }] },
  { id: 'api_credential', label: 'API Credentials', color: 'from-cyan-300 to-teal-600', primary: true, sections: [{ id: 'main', title: null, fields: [f('username', 'username'), f('credential', 'credential', 'concealed'), f('type', 'type'), f('hostname', 'hostname', 'url'), f('expires', 'expires', 'date')] }] },
  { id: 'document', label: 'Document', color: 'from-blue-300 to-indigo-600', primary: true, sections: [] },
  { id: 'ssh_key', label: 'SSH Key', color: 'from-slate-300 to-slate-600', primary: false, sections: [{ id: 'main', title: null, fields: [f('private_key', 'private key', 'concealed'), f('public_key', 'public key', 'multiline'), f('fingerprint', 'fingerprint')] }] },
  { id: 'wifi', label: 'Wi-Fi', color: 'from-violet-300 to-purple-600', primary: false, sections: [{ id: 'main', title: null, fields: [f('ssid', 'network name'), f('password', 'wireless password', 'concealed'), f('security', 'security')] }] },
];
export const categoryById = (id: ItemCategory) => CATEGORIES.find(c => c.id === id)!;
export function buildSections(cat: ItemCategory, values: Record<string, string>): Section[] {
  return categoryById(cat).sections.map(s => ({ id: s.id, title: s.title, fields: s.fields.map(fl => ({ ...fl, value: values[`${s.id}.${fl.id}`]?.trim() ? values[`${s.id}.${fl.id}`] : null })) }))
    .map(s => ({ ...s, fields: s.fields.filter(x => x.value !== null) })).filter(s => s.fields.length > 0);
}
export function filterCategories(q: string) { const s = q.trim().toLowerCase(); return s ? CATEGORIES.filter(c => c.label.toLowerCase().includes(s) || c.id.includes(s)) : CATEGORIES; }
/** If the query looks like a website, suggest a Login. */
export function domainSuggestion(q: string): { url: string; title: string } | null {
  const s = q.trim().toLowerCase().replace(/^https?:\/\//, '').replace(/\/.*$/, '');
  if (!/^([a-z0-9-]+\.)+[a-z]{2,}$/.test(s)) return null;
  const host = s.replace(/^www\./, ''); const parts = host.split('.');
  const name = parts.length > 2 && parts[parts.length - 2].length <= 3 ? parts[parts.length - 3] : parts[parts.length - 2];
  return { url: `https://${host}`, title: name.charAt(0).toUpperCase() + name.slice(1) };
}
