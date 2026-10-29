/**
 * Client-side TOTP preview for the item editor. Mirrors `gitpass-core`'s `totp::parse`:
 * an `otpauth://totp/...` URI or a raw base32 secret, HMAC-SHA1/256/512. The secret never
 * leaves the webview; the preview is only a sanity check while editing.
 */
export type TotpAlgo = 'SHA-1' | 'SHA-256' | 'SHA-512';
export interface TotpSpec { secret: Uint8Array; digits: number; period: number; algo: TotpAlgo }

export function base32Decode(s: string): Uint8Array {
  const out: number[] = [];
  let buf = 0, bits = 0;
  for (const ch of s) {
    if (ch === ' ' || ch === '-' || ch === '=') continue;
    const c = ch.toUpperCase();
    let v: number;
    if (c >= 'A' && c <= 'Z') v = c.charCodeAt(0) - 65;
    else if (c >= '2' && c <= '7') v = c.charCodeAt(0) - 50 + 26;
    else throw new Error('invalid base32 character');
    buf = (buf << 5) | v; bits += 5;
    if (bits >= 8) { bits -= 8; out.push((buf >> bits) & 0xff); buf &= (1 << bits) - 1; }
  }
  if (out.length === 0) throw new Error('empty TOTP secret');
  return Uint8Array.from(out);
}

/** Returns the parsed spec, or null when the value is not a usable TOTP secret / URI. */
export function parseTotp(input: string): TotpSpec | null {
  const s = input.trim();
  if (!s) return null;
  try {
    let digits = 6, period = 30, algo: TotpAlgo = 'SHA-1';
    let secret: Uint8Array;
    if (/^otpauth:\/\//i.test(s)) {
      const rest = s.slice(10);
      if (!/^totp[/?]/i.test(rest)) return null;
      const q = rest.split('?')[1];
      if (q === undefined) return null;
      let sec: Uint8Array | null = null;
      for (const kv of q.split('&')) {
        const i = kv.indexOf('=');
        const k = (i < 0 ? kv : kv.slice(0, i)).toLowerCase();
        const v = decodeURIComponent(i < 0 ? '' : kv.slice(i + 1));
        if (k === 'secret') sec = base32Decode(v);
        else if (k === 'digits') { if (!/^\d+$/.test(v)) return null; digits = Number(v); }
        else if (k === 'period') { if (!/^\d+$/.test(v)) return null; period = Number(v); }
        else if (k === 'algorithm') {
          const a = v.toUpperCase();
          if (a === 'SHA1') algo = 'SHA-1'; else if (a === 'SHA256') algo = 'SHA-256'; else if (a === 'SHA512') algo = 'SHA-512'; else return null;
        }
      }
      if (!sec) return null;
      secret = sec;
    } else {
      secret = base32Decode(s);
    }
    if (digits < 6 || digits > 10 || period <= 0) return null;
    return { secret, digits, period, algo };
  } catch { return null; }
}

/** RFC 6238 code for `nowMs` (defaults to the current time). */
export async function totpCode(spec: TotpSpec, nowMs: number = Date.now()): Promise<string> {
  const counter = BigInt(Math.floor(nowMs / 1000 / spec.period));
  const msg = new Uint8Array(8);
  new DataView(msg.buffer).setBigUint64(0, counter);
  const key = await crypto.subtle.importKey('raw', spec.secret as BufferSource, { name: 'HMAC', hash: spec.algo }, false, ['sign']);
  const mac = new Uint8Array(await crypto.subtle.sign('HMAC', key, msg));
  const off = mac[mac.length - 1] & 0x0f;
  const bin = ((mac[off] & 0x7f) << 24) | (mac[off + 1] << 16) | (mac[off + 2] << 8) | mac[off + 3];
  return (BigInt(bin) % 10n ** BigInt(spec.digits)).toString().padStart(spec.digits, '0');
}

export const totpRemaining = (spec: TotpSpec, nowMs: number = Date.now()) =>
  spec.period - (Math.floor(nowMs / 1000) % spec.period);
