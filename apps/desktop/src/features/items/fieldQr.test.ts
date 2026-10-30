import { describe, expect, it } from 'vitest';
import type { Field } from '../../api/types';
import { buildTotpUri, isQrSupportedField } from './fieldQr';

describe('isQrSupportedField', () => {
  it('supports password fields', () => {
    const pw: Field = { id: 'password', label: 'password', kind: 'concealed', value: null };
    expect(isQrSupportedField(pw, 'login')).toBe(true);

    const pwByLabel: Field = { id: 'pass_custom', label: 'Password', kind: 'concealed', value: null };
    expect(isQrSupportedField(pwByLabel, 'login')).toBe(true);
  });

  it('supports username fields', () => {
    const user: Field = { id: 'username', label: 'username', kind: 'text', value: 'alice' };
    expect(isQrSupportedField(user, 'login')).toBe(true);

    const userByLabel: Field = { id: 'user_custom', label: 'Username', kind: 'text', value: 'alice' };
    expect(isQrSupportedField(userByLabel, 'login')).toBe(true);
  });

  it('supports website and URL fields', () => {
    const site: Field = { id: 'website', label: 'website', kind: 'url', value: 'https://example.com' };
    expect(isQrSupportedField(site, 'login')).toBe(true);

    const customUrl: Field = { id: 'console_url', label: 'Console', kind: 'url', value: 'https://console.example.com' };
    expect(isQrSupportedField(customUrl, 'server')).toBe(true);
  });

  it('supports one-time-code (TOTP) setup key fields', () => {
    const totpKind: Field = { id: 'otp', label: 'one-time password', kind: 'totp', value: null };
    expect(isQrSupportedField(totpKind, 'login')).toBe(true);

    const totpId: Field = { id: 'totp', label: 'Authenticator Key', kind: 'text', value: 'JBSWY3DPEHPK3PXP' };
    expect(isQrSupportedField(totpId, 'login')).toBe(true);
  });

  it('supports API credential secrets', () => {
    const apiKey: Field = { id: 'credential', label: 'credential', kind: 'concealed', value: null };
    expect(isQrSupportedField(apiKey, 'api_credential')).toBe(true);

    const customApiSecret: Field = { id: 'secret_token', label: 'API Secret', kind: 'concealed', value: null };
    expect(isQrSupportedField(customApiSecret, 'api_credential')).toBe(true);
  });

  it('supports custom text and concealed fields', () => {
    const customText: Field = { id: 'sec_answer', label: 'Security Answer', kind: 'text', value: 'blue' };
    expect(isQrSupportedField(customText, 'login')).toBe(true);

    const customConcealed: Field = { id: 'recovery_key', label: 'Recovery Key', kind: 'concealed', value: null };
    expect(isQrSupportedField(customConcealed, 'login')).toBe(true);
  });

  it('rejects credit card standard fields (number, cvv, pin, cardholder, expiry)', () => {
    const ccNum: Field = { id: 'number', label: 'number', kind: 'concealed', value: null };
    expect(isQrSupportedField(ccNum, 'credit_card')).toBe(false);

    const ccCvv: Field = { id: 'cvv', label: 'verification number', kind: 'concealed', value: null };
    expect(isQrSupportedField(ccCvv, 'credit_card')).toBe(false);

    const ccPin: Field = { id: 'pin', label: 'PIN', kind: 'concealed', value: null };
    expect(isQrSupportedField(ccPin, 'credit_card')).toBe(false);

    const ccHolder: Field = { id: 'cardholder', label: 'cardholder name', kind: 'text', value: 'Alice' };
    expect(isQrSupportedField(ccHolder, 'credit_card')).toBe(false);
  });

  it('rejects bank account standard fields (account number, routing number, pin)', () => {
    const acctNum: Field = { id: 'account_number', label: 'account number', kind: 'text', value: '12345678' };
    expect(isQrSupportedField(acctNum, 'bank_account')).toBe(false);

    const routing: Field = { id: 'routing_number', label: 'routing number', kind: 'text', value: '987654321' };
    expect(isQrSupportedField(routing, 'bank_account')).toBe(false);

    const bankPin: Field = { id: 'pin', label: 'telephone PIN', kind: 'concealed', value: null };
    expect(isQrSupportedField(bankPin, 'bank_account')).toBe(false);
  });

  it('rejects identity standard fields and notes', () => {
    const birth: Field = { id: 'birth', label: 'birth date', kind: 'date', value: '1990-01-01' };
    expect(isQrSupportedField(birth, 'identity')).toBe(false);

    const address: Field = { id: 'address', label: 'address', kind: 'multiline', value: '123 Main St' };
    expect(isQrSupportedField(address, 'identity')).toBe(false);

    const note: Field = { id: 'note', label: 'note', kind: 'multiline', value: 'secret notes' };
    expect(isQrSupportedField(note, 'secure_note')).toBe(false);
  });
});

describe('buildTotpUri', () => {
  it('constructs an otpauth://totp/ URI from a raw base32 secret with title and username', () => {
    const uri = buildTotpUri('JBSWY3DPEHPK3PXP', 'GitHub', 'octocat');
    expect(uri).toBe('otpauth://totp/GitHub%3Aoctocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub');
  });

  it('constructs an otpauth://totp/ URI with only item title when username is absent', () => {
    const uri = buildTotpUri('JBSWY3DPEHPK3PXP', 'Example');
    expect(uri).toBe('otpauth://totp/Example?secret=JBSWY3DPEHPK3PXP&issuer=Example');
  });

  it('strips spaces and dashes from raw base32 secret', () => {
    const uri = buildTotpUri('JBSW-Y3DP EHPK-3PXP', 'Acme Corp');
    expect(uri).toBe('otpauth://totp/Acme%20Corp?secret=JBSWY3DPEHPK3PXP&issuer=Acme+Corp');
  });

  it('preserves already full otpauth:// URIs verbatim', () => {
    const full = 'otpauth://totp/Custom:user?secret=JBSWY3DPEHPK3PXP&digits=8&period=60&algorithm=SHA256';
    expect(buildTotpUri(full, 'IgnoredTitle', 'IgnoredUser')).toBe(full);
  });

  it('defaults label and issuer to gitpass when title is omitted', () => {
    const uri = buildTotpUri('JBSWY3DPEHPK3PXP');
    expect(uri).toBe('otpauth://totp/gitpass?secret=JBSWY3DPEHPK3PXP&issuer=gitpass');
  });
});
