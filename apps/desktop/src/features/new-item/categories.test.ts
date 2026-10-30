import { describe, expect, it } from 'vitest';
import { CATEGORIES, buildSections, domainSuggestion, filterCategories } from './categories';
import { normaliseTotp } from './CreateForm';
describe('categories', () => {
  it('covers contract categories, with Bank Account and Server', () => {
    expect(CATEGORIES.map(c => c.id).sort()).toEqual(['api_credential', 'bank_account', 'credit_card', 'document', 'identity', 'login', 'secure_note', 'server', 'ssh_key', 'wifi']);
    expect(CATEGORIES.filter(c => c.primary).map(c => c.label)).toEqual(['Login', 'Secure Note', 'Credit Card', 'Identity', 'API Credentials', 'Document', 'Bank Account', 'Server']);
  });
  it('bank account and server templates have expected fields and concealed kinds', () => {
    const bank = CATEGORIES.find(c => c.id === 'bank_account')!.sections[0].fields;
    expect(bank.find(x => x.id === 'account_number')?.kind).toBe('concealed');
    expect(bank.find(x => x.id === 'pin')?.kind).toBe('concealed');
    expect(bank.find(x => x.id === 'routing_number')?.kind).toBe('text');

    const srv = CATEGORIES.find(c => c.id === 'server')!.sections[0].fields;
    expect(srv.find(x => x.id === 'password')?.kind).toBe('concealed');
    expect(srv.find(x => x.id === 'url')?.kind).toBe('url');
    expect(srv.find(x => x.id === 'admin_console_url')?.kind).toBe('url');
  });
  it('login template has concealed password and totp', () => {
    const f = CATEGORIES[0].sections[0].fields;
    expect(f.find(x => x.id === 'password')?.kind).toBe('concealed');
    expect(f.find(x => x.id === 'otp')?.kind).toBe('totp');
  });
  it('buildSections drops empty fields/sections', () => {
    const s = buildSections('identity', { 'name.first': 'Sam', 'name.last': ' ', 'contact.email': 'sam@example.com' });
    expect(s).toEqual([{ id: 'name', title: 'Identification', fields: [{ id: 'first', label: 'first name', kind: 'text', value: 'Sam' }] },
      { id: 'contact', title: 'Contact', fields: [{ id: 'email', label: 'email', kind: 'email', value: 'sam@example.com' }] }]);
  });
  it('search + domain suggestion', () => {
    expect(filterCategories('card').map(c => c.id)).toEqual(['credit_card']);
    expect(domainSuggestion('github.com')).toEqual({ url: 'https://github.com', title: 'Github' });
    expect(domainSuggestion('https://www.example.co.uk/login')).toEqual({ url: 'https://example.co.uk', title: 'Example' });
    expect(domainSuggestion('login')).toBeNull();
  });
  it('totp normalisation', () => {
    expect(normaliseTotp('jbsw y3dp ehpk 3pxp')).toBe('JBSWY3DPEHPK3PXP');
    expect(normaliseTotp('otpauth://totp/x?secret=JBSWY3DPEHPK3PXP')).toMatch(/^otpauth/);
    expect(normaliseTotp('hello!')).toBeNull();
  });
});
