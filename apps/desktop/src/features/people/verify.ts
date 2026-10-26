// Out-of-band verification helpers. The fingerprint itself is computed in Rust (`fingerprint_of`),
// the same value the recipient's gitpass shows under People → Your identity.
/** Cheap client-side shape check (base64 of a 32-byte X25519 key, optional `gitpass-x25519:` prefix,
 * or the CLI's JSON export). Rust's parser is the real authority. */
export function looksLikePublicKey(k: string): boolean {
  const s = k.trim();
  if (s.startsWith('{')) return /"public"\s*:\s*"/.test(s);
  return /^(gitpass-x25519:)?[A-Za-z0-9+/]{43}=$/.test(s);
}
/** Split a fingerprint into 4-char groups for reading aloud. */
export function verificationGroups(fpr: string): string[] { const s = fpr.trim().replace(/\s+/g, ''); const out: string[] = []; for (let i = 0; i < s.length; i += 4) out.push(s.slice(i, i + 4)); return out; }
