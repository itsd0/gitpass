import { describe, expect, it } from 'vitest';
import { base32Decode, parseTotp, totpCode, totpRemaining } from './totpPreview';

// RFC 6238 appendix B secrets, base32 of the ASCII keys.
const SHA1_SECRET = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ';
const SHA256_SECRET = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA';
const SHA512_SECRET = 'GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA';

describe('totpPreview', () => {
  it('decodes base32 ignoring case, spaces and padding', () => {
    expect(Array.from(base32Decode('mzxw 6ytb-oi======'))).toEqual(Array.from(new TextEncoder().encode('foobar')));
  });

  it('matches RFC 6238 vectors for SHA1, SHA256 and SHA512 (8 digits, t=59)', async () => {
    const t = 59_000;
    expect(await totpCode(parseTotp(`otpauth://totp/x?secret=${SHA1_SECRET}&digits=8`)!, t)).toBe('94287082');
    expect(await totpCode(parseTotp(`otpauth://totp/x?secret=${SHA256_SECRET}&digits=8&algorithm=SHA256`)!, t)).toBe('46119246');
    expect(await totpCode(parseTotp(`otpauth://totp/x?secret=${SHA512_SECRET}&digits=8&algorithm=SHA512`)!, t)).toBe('90693936');
  });

  it('accepts a bare base32 secret and an unlabeled otpauth URI', async () => {
    expect(await totpCode(parseTotp(SHA1_SECRET)!, 59_000)).toBe('287082');
    expect(parseTotp(`otpauth://totp?secret=${SHA1_SECRET}`)).not.toBeNull();
  });

  it('rejects junk, hotp links, bad digits and missing secrets', () => {
    for (const bad of ['', '   ', 'not base32!', 'otpauth://hotp/x?secret=ABCDEFGH', 'otpauth://totp/x', `otpauth://totp/x?secret=${SHA1_SECRET}&digits=3`, `otpauth://totp/x?secret=${SHA1_SECRET}&period=0`, `otpauth://totp/x?secret=${SHA1_SECRET}&algorithm=MD5`]) {
      expect(parseTotp(bad)).toBeNull();
    }
  });

  it('reports seconds remaining in the period', () => {
    const spec = parseTotp(SHA1_SECRET)!;
    expect(totpRemaining(spec, 59_000)).toBe(1);
    expect(totpRemaining(spec, 60_000)).toBe(30);
  });
});
