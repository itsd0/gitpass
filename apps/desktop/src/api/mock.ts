// Canonical in-browser mock for `npm run dev` outside Tauri. ALL DATA IS FAKE.
// Concealed values are kept in a private map and only returned by reveal_field,
// mirroring the Rust security model (detail payloads carry value: null).
import type * as T from './types';
import { cardDigits, cardLabel, cardMask, secretHint } from '../lib/cardMask';

const delay = <R>(v: R, ms = 60): Promise<R> => new Promise((r) => setTimeout(() => r(structuredClone(v)), ms));
let seq = 1000;
const uid = (p: string) => `${p}_${(seq++).toString(36)}`;

interface Rec { detail: T.ItemDetail; secrets: Record<string, string>; versions: T.ItemDetail[] }

// Fake keys in the real wire format (base64 of 32 bytes, optional `gitpass-x25519:` prefix).
const PUB_RE = /^(gitpass-x25519:)?[A-Za-z0-9+/]{43}=$/;
/** Mock stand-in for blake3 fingerprint: deterministic 32 hex chars (NOT the real algorithm). */
export function mockFingerprint(publicKey: string): string {
  const k = publicKey.trim().replace(/^gitpass-x25519:/, ''); let out = '';
  for (let r = 0; r < 4; r++) { let h = 0x811c9dc5 ^ r; for (const c of k) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193) >>> 0; out += h.toString(16).padStart(8, '0'); }
  return out;
}
const SELF_KEY = 'RkFLRS1kZW1vLXB1YmxpYy1rZXktZXhhbXBsZTAwMDA=';
const SELF: T.Member = { fingerprint: mockFingerprint(SELF_KEY), label: 'You (Demo User)', isSelf: true };
const vaults: T.VaultInfo[] = [
  { id: 'v_private', name: 'Private', description: 'Personal items', itemCount: 0, shared: false, members: [SELF], icon: 'lock', color: '#6366f1' },
  { id: 'v_work', name: 'Work (Example Corp)', description: 'Fake work credentials', itemCount: 0, shared: false, members: [SELF], icon: 'briefcase', color: '#0ea5e9' },
  { id: 'v_family', name: 'Family Shared', description: 'Shared with demo family', itemCount: 0, shared: true, icon: 'users', color: '#ec4899',
    members: [SELF, { fingerprint: mockFingerprint('QWxleC1FeGFtcGxlLWZha2Uta2V5LTAwMDAwMDAwMDA='), label: 'Alex Example', isSelf: false }, { fingerprint: mockFingerprint('U2FtLVNhbXBsZS1mYWtlLWtleS0wMDAwMDAwMDAwMDA='), label: 'Sam Sample', isSelf: false }] },
];

const db = new Map<string, Rec>();
const f = (label: string, kind: T.FieldKind, value: string): [T.Field, string | null] => {
  const id = uid('f');
  const hidden = kind === 'concealed' || kind === 'totp';
  const masked = kind === 'concealed' && label === 'number' && cardDigits(value).length >= 8 ? cardMask(value) : undefined;
  return [{ id, label, kind, value: hidden ? null : value, ...(masked ? { masked } : {}) }, hidden ? value : null];
};

// 0-100 like gui-core `strength` (rough stand-in; real scoring is entropy-based in Rust).
function strength(pw: string): number { return strength4(pw) * 25; }
function strength4(pw: string): number {
  let s = 0;
  if (pw.length >= 8) s++; if (pw.length >= 14) s++;
  if (/[A-Z]/.test(pw) && /[a-z]/.test(pw)) s++;
  if (/\d/.test(pw) && /[^A-Za-z0-9]/.test(pw)) s++;
  return Math.min(4, s);
}

function add(vaultId: string, category: T.ItemCategory, title: string, subtitle: string, daysAgo: number,
  fields: [string, T.FieldKind, string][], extra: Partial<T.ItemDetail> = {}) {
  const secrets: Record<string, string> = {};
  const fs = fields.map(([l, k, v]) => { const [fld, s] = f(l, k, v); if (s !== null) secrets[fld.id] = s; return fld; });
  const pwField = fs.find((x) => x.kind === 'concealed' && /password/i.test(x.label));
  const ts = new Date(Date.UTC(2026, 9, 25, 12) - daysAgo * 86400000).toISOString();
  const d: T.ItemDetail = {
    id: uid('i'), vaultId, category, title, subtitle, favorite: false, archived: false, deletedAt: null, tags: [],
    updatedAt: ts, createdAt: new Date(Date.parse(ts) - 30 * 86400000).toISOString(), url: null, changePasswordUrl: changePwUrl(extra.url),
    sections: [{ id: uid('s'), title: null, fields: fs }], notes: null, attachments: [],
    history: [{ version: 1, updatedAt: ts }], passwordStrength: pwField ? strength(secrets[pwField.id]) : null, passwordChangedAt: pwField ? ts : null,
    hasTotp: fs.some((x) => x.kind === 'totp'), ignoredFindings: [], ...extra,
  };
  db.set(d.id, { detail: d, secrets, versions: [] });
  return d;
}

// ---- fixtures (~40 items, clearly fake) ----
const logins: [string, string, string, string, number][] = [
  ['Example Bank', 'demo.user@example.com', 'https://bank.example.com', 'Tr0ub4dor&3-fake', 1],
  ['GitHub (demo)', 'octo-demo', 'https://github.example.com', 'hunter2', 2],
  ['Acme Mail', 'demo@acme.test', 'https://mail.acme.test', 'correct-horse-battery-staple-9!', 3],
  ['Fake Streaming Co', 'family@example.org', 'https://stream.example.org', 'password123', 5],
  ['Sample Shop', 'shopper@example.com', 'https://shop.example.com', 'S@mple!Shop#2026xyz', 8],
  ['Demo Cloud Console', 'admin@example.dev', 'https://console.example.dev', 'Zx9!qLm#77pWv2', 12],
  ['Test Airline', 'flyer123', 'https://air.example.net', 'password123', 20],
  ['Placeholder Forum', 'lurker', 'http://forum.example.io', 'qwerty', 34],
  ['Example Social', 'demo_social', 'https://social.example.com', 'N0t-A-Real-Pass!!', 41],
  ['Mock Utility Co', 'acct-000123', 'https://utility.example.com', 'Utility#Fake#42', 60],
  ['Sandbox VPN', 'vpnuser', 'https://vpn.example.com', 'v9$Kx!2mQ@zz', 75],
  ['Dummy Insurance', 'policy-holder', 'https://insure.example.com', 'Ins!Demo2025', 95],
];
logins.forEach(([t, u, url, pw, d], i) => {
  const fields: [string, T.FieldKind, string][] = [['username', 'text', u], ['password', 'concealed', pw], ['website', 'url', url]];
  if (i % 3 === 0) fields.push(['one-time password', 'totp', 'JBSWY3DPEHPK3PXP']);
  add(['v_private', 'v_work', 'v_family'][i % 3], 'login', t, u, d, fields, { url, favorite: i < 3, tags: i % 4 === 0 ? ['finance'] : i % 4 === 1 ? ['dev'] : [] });
});
const cards: [string, string, number][] = [
  ['Example Bank Visa', '4111 1111 1111 1111', 0], ['Demo Credit Union Visa', '4012 8888 8888 1881', 4], ['Fake Rewards Mastercard', '5555 5555 5555 4444', 9],
  ['Test Travel Amex', '3782 822463 10005', 15], ['Sample Debit Visa', '4000 0566 5566 5556', 40], ['Mock Business Card', '5105 1051 0510 5100', 70],
];
cards.forEach(([t, n, d], i) => add(i % 2 ? 'v_family' : 'v_private', 'credit_card', t, cardLabel(n), d, [
  ['cardholder name', 'text', 'Demo User'], ['number', 'concealed', n], ['verification number', 'concealed', '123'], ['expiry date', 'month_year', `203${i}/0${(i % 9) + 1}`],
], { tags: ['finance'], favorite: i === 0 }));
['Wi-Fi router setup', 'Recipe: fake lasagna', 'Locker combination', 'Meeting notes (sample)', 'Gift ideas', 'Server runbook (demo)'].forEach((t, i) =>
  add(i % 2 ? 'v_work' : 'v_private', 'secure_note', t, 'Secure note', 6 + i * 11, [], { notes: `This is a fake secure note: ${t}.\nNothing real here.`, tags: i === 5 ? ['dev'] : [] }));
[['Demo User', 'demo.user@example.com'], ['Alex Example', 'alex@example.com'], ['Sam Sample', 'sam@example.com']].forEach(([n, e], i) =>
  add(i ? 'v_family' : 'v_private', 'identity', `${n} identity`, n, 7 + i * 20, [
    ['first name', 'text', n.split(' ')[0]], ['last name', 'text', n.split(' ')[1]], ['email', 'email', e], ['phone', 'phone', '+1 555 0100'], ['birth date', 'date', '1990-01-01'],
  ]));
['Stripe test key (fake)', 'OpenWeather demo token', 'Internal API (Example Corp)', 'CI deploy token (fake)', 'Maps sandbox key'].forEach((t, i) =>
  add('v_work', 'api_credential', t, `key ••• ${1000 + i * 7}`, 3 + i * 9, [
    ['username', 'text', `svc-${i}`], ['credential', 'concealed', `sk_test_FAKE${i}xxxxxxxxxxxxxxxx`], ['hostname', 'url', `https://api${i}.example.com`], ['expires', 'date', '2027-12-31'],
  ], { tags: ['dev'] }));
['Passport scan (fake)', 'Lease agreement (sample)', 'Tax return 2025 (demo)'].forEach((t, i) =>
  add(i ? 'v_family' : 'v_private', 'document', t, 'PDF document', 10 + i * 30, [], {
    attachments: [{ id: uid('a'), name: `${t.split(' ')[0].toLowerCase()}.pdf`, size: 120000 + i * 54321, mime: 'application/pdf' }] }));
['Deploy key (demo server)', 'Personal laptop key'].forEach((t, i) =>
  add('v_work', 'ssh_key', t, 'ED25519 SHA256:FAKE' + i, 14 + i * 25, [
    ['private key', 'concealed', '-----BEGIN OPENSSH PRIVATE KEY-----\nFAKEFAKEFAKE\n-----END OPENSSH PRIVATE KEY-----'], ['public key', 'multiline', `ssh-ed25519 AAAAC3NzaFAKE${i} demo@example`], ['fingerprint', 'text', 'SHA256:FAKE' + i],
  ]));
['Home Wi-Fi (Example-Net)', 'Office Guest Wi-Fi', 'Cabin Wi-Fi'].forEach((t, i) =>
  add(i === 1 ? 'v_work' : 'v_family', 'wifi', t, `Example-Net-${i}`, 2 + i * 33, [['network name', 'text', `Example-Net-${i}`], ['wireless password', 'concealed', `wifi-pass-${i}-fake`], ['security', 'text', 'WPA3']]));
// a couple archived / deleted for those views
{ const a = [...db.values()][5].detail; a.archived = true; const b = [...db.values()][22].detail; b.deletedAt = new Date(Date.now() - 29 * 86400000).toISOString(); }

add('v_private', 'passkey', 'webauthn.example.invalid', 'demo@example.invalid', 6, [['site', 'text', 'webauthn.example.invalid'], ['user name', 'text', 'demo@example.invalid'], ['credential id', 'text', 'FAKE-cred-id'], ['private key', 'concealed', 'FAKE'], ['sign count', 'text', '3']]);
let settings: T.Settings = { autoLockMinutes: 10, clipboardClearSecs: 90, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false, lockOnSleep: true, lockOnScreenLock: true, lockOnUserSwitch: true, hideFromScreenRecording: true, generatorMode: 'random', generatorLength: 20, generatorDigits: true, generatorSymbols: true, showWebsiteIcons: false, renderNotesMarkdown: true, showMenuBarIcon: true, listDensity: 'comfortable', rememberGeneratedPasswords: true, watchtowerUnusedMonths: 12, lastSeenVersion: undefined };
let mockGeneratorHistory: T.GeneratorHistoryEntry[] = [
  { id: "gh-1", value: "FAKE-mock-pw-1", createdAt: Math.floor(Date.now() / 1000) - 300, source: "desktop", host: null },
  { id: "gh-2", value: "FAKE-mock-pw-2", createdAt: Math.floor(Date.now() / 1000) - 7200, source: "extension", host: "github.com" },
];
let mockSshActivity: T.SshActivityEntry[] = [
  { time: new Date(Date.now() - 45_000).toISOString(), keyTitle: 'Deploy key (demo server)', fingerprint: 'SHA256:FAKE0', keyType: 'ssh-ed25519', algorithm: 'ed25519', result: 'signed', client: 'git-upload-pack', pid: 4242 },
  { time: new Date(Date.now() - 8 * 60_000).toISOString(), keyTitle: 'Personal laptop key', fingerprint: 'SHA256:FAKE1', keyType: 'ssh-rsa', algorithm: 'rsa-sha2-256', result: 'signed', client: 'ssh', pid: 4177 },
  { time: new Date(Date.now() - 42 * 60_000).toISOString(), keyTitle: 'Personal laptop key', fingerprint: 'SHA256:FAKE1', keyType: 'ssh-rsa', algorithm: 'ssh-rsa', result: 'refused', client: 'ssh', pid: 4102 },
];
let onboarding: T.Onboarding = { extensionInstalled: false, emergencyKitSaved: false, repoConnected: true, imported: false, dismissed: false };
let unlocked = false;
const devices: T.DeviceInfo[] = [
  { id: 'd1', name: 'Demo MacBook (this device)', lastSeen: new Date().toISOString(), revoked: false, current: true },
  { id: 'd2', name: 'Example Linux Desktop', lastSeen: '2026-10-20T10:00:00Z', revoked: false, current: false },
];

const summary = (d: T.ItemDetail): T.ItemSummary => {
  const { sections: _s, notes: _n, attachments: _a, history: _h, passwordStrength: _p, hasTotp: _t, findings: _fi, passwordChangedAt: _pca, ...s } = d;
  const flds = d.sections?.flatMap((sec) => sec.fields) ?? [];
  let expiry: string | null = null;
  if (d.category === "credit_card") {
    const fld = flds.find((x) => x.id === "expiry" || x.label === "expiry date" || x.kind === "month_year");
    expiry = fld?.value ?? null;
  } else if (d.category === "api_credential") {
    const fld = flds.find((x) => x.id === "expires" || x.label === "expires" || x.kind === "date");
    expiry = fld?.value ?? null;
  }
  return { ...s, ...(expiry ? { expiry } : {}), ignoredFindings: d.ignoredFindings ?? [] };
};
const get = (id: string) => { const r = db.get(id); if (!r) throw new Error(`item not found: ${id}`); return r; };
const vaultInfo = () => vaults.map((v) => ({ ...v, itemCount: [...db.values()].filter((r) => r.detail.vaultId === v.id && !r.detail.deletedAt).length }));
const touch = (r: Rec) => {
  r.versions.push(structuredClone(r.detail));
  const now = new Date().toISOString(); r.detail.updatedAt = now;
  const nextVer = (r.detail.version ?? r.detail.history.length) + 1;
  r.detail.version = nextVer;
  r.detail.history = [...r.detail.history, { version: nextVer, updatedAt: now }];
};
function annotateMockDiff(snap: T.ItemDetail, cur: T.ItemDetail) {
  const curFields = cur.sections.flatMap((s) => s.fields);
  const snapFields = snap.sections.flatMap((s) => s.fields);
  for (const s of snap.sections) {
    for (const f of s.fields) {
      const cf = curFields.find((x) => x.id === f.id);
      if (cf) {
        if (f.value !== cf.value) f.diff = 'changed';
      } else {
        f.diff = 'removed';
      }
    }
  }
  const added = curFields.filter((cf) => !snapFields.some((sf) => sf.id === cf.id)).map((cf) => ({
    ...structuredClone(cf),
    diff: 'added' as const,
  }));
  if (added.length > 0) {
    if (snap.sections[0]) {
      snap.sections[0].fields.push(...added);
    } else {
      snap.sections.push({ id: 'added', title: null, fields: added });
    }
  }
}
const subtitleFor = (input: T.ItemInput): string => {
  const flds = input.sections.flatMap((s) => s.fields);
  if (input.category === 'credit_card') { const n = flds.find((x) => x.id === 'number' || x.label === 'number'); return n?.value ? cardLabel(n.value) : ''; }
  if (input.category === 'bank_account') {
    const num = flds.find((x) => x.id === 'account_number' || x.label === 'account number');
    const b = flds.find((x) => x.id === 'bank_name' || x.label === 'bank name');
    const t = flds.find((x) => x.id === 'account_type' || x.label === 'account type');
    const l4 = num?.value ? `•••• ${num.value.slice(-4)}` : '';
    if (t?.value && l4) return `${t.value} ${l4}`;
    if (b?.value && l4) return `${b.value} ${l4}`;
    if (l4) return l4;
    return b?.value ?? '';
  }
  if (input.category === 'server') {
    const u = flds.find((x) => x.id === 'username' || x.label === 'username')?.value;
    const h = (flds.find((x) => x.id === 'url' || x.label.toLowerCase().includes('url') || x.label === 'hostname')?.value) || input.url;
    if (u && h) return `${u}@${h}`;
    return u || h || '';
  }
  if (input.category === 'api_credential') {
    const cred = flds.find((x) => x.kind === 'concealed' && (x.id === 'credential' || x.id === 'key' || x.id === 'api_key' || x.id === 'secret'))
      || flds.find((x) => x.kind === 'concealed' && x.value);
    if (cred?.value) return secretHint(cred.value);
  }
  const u = flds.find((x) => x.kind === 'text' || x.kind === 'email');
  return u?.value ?? '';
};
function applyInput(r: Rec, input: T.ItemInput) {
  const secrets: Record<string, string> = {};
  const sections = input.sections.map((s) => ({ ...s, id: s.id || uid('s'), fields: s.fields.map((x) => {
    const id = x.id || uid('f'); const hidden = x.kind === 'concealed' || x.kind === 'totp';
    // null concealed value on update = "unchanged"
    if (hidden) { const v = x.value ?? r.secrets[id]; if (v != null) secrets[id] = v; }
    const masked = input.category === 'credit_card' && hidden && (x.id === 'number' || x.label === 'number') && cardDigits(x.value ?? r.secrets[id]).length >= 8
      ? cardMask(x.value ?? r.secrets[id])
      : input.category === 'api_credential' && hidden && (x.value ?? r.secrets[id])
      ? secretHint(x.value ?? r.secrets[id])
      : undefined;
    return { ...x, id, value: hidden ? null : x.value, ...(masked ? { masked } : {}) };
  }) }));
  r.secrets = secrets;
  Object.assign(r.detail, { vaultId: input.vaultId, category: input.category, title: input.title, sections, notes: input.notes ?? null,
    tags: input.tags ?? [], favorite: input.favorite ?? r.detail.favorite, url: input.url ?? null, changePasswordUrl: changePwUrl(input.url),
    subtitle: subtitleFor(input) || r.detail.subtitle,
    hasTotp: sections.some((s) => s.fields.some((x) => x.kind === 'totp')),
    ...(input.relatedItemIds !== undefined ? { relatedItemIds: [...input.relatedItemIds] } : {}) });
  const pw = sections.flatMap((s) => s.fields).find((x) => x.kind === 'concealed');
  r.detail.passwordStrength = pw && secrets[pw.id] ? strength(secrets[pw.id]) : null;
  if (pw && secrets[pw.id]) { if (!r.detail.passwordChangedAt) r.detail.passwordChangedAt = r.detail.updatedAt; } else { r.detail.passwordChangedAt = null; }
}
/** Rough browser-mock copy of core `change_password_url` (the real derivation runs in Rust). */
function changePwUrl(raw: string | null | undefined): string | undefined {
  const s = (raw ?? '').trim();
  const m = /^https?:\/\//i.exec(s);
  if (!m) return undefined;
  const authority = s.slice(m[0].length).split(/[/?#]/)[0];
  const hostPort = authority.split('@').pop() ?? '';
  if (hostPort.startsWith('[')) return undefined;
  const colon = hostPort.indexOf(':');
  const host = (colon >= 0 ? hostPort.slice(0, colon) : hostPort).replace(/\.+$/, '').toLowerCase();
  const port = colon >= 0 ? hostPort.slice(colon + 1) : '';
  if (!host || host.split('.').every((l) => /^\d+$/.test(l))) return undefined;
  if (host === 'localhost' || host.endsWith('.localhost') || host === 'local' || host.endsWith('.local')) return undefined;
  if (!host.split('.').every((l) => /^[a-z0-9_](?:[a-z0-9_-]*[a-z0-9_])?$/.test(l))) return undefined;
  if (port && !/^\d+$/.test(port)) return undefined;
  const n = port ? Number(port) : 0;
  if (port && (n < 1 || n > 65535)) return undefined;
  return `https://${host}${port ? `:${n}` : ''}/.well-known/change-password`;
}
/** Rough browser-mock findings for the item-detail banner (the real audit runs in Rust). */
function mockFindings(id: string): T.FindingInfo[] {
  const r = db.get(id); if (!r) return [];
  const d = r.detail;
  if (d.category === 'passkey') return [];
  const flds = d.sections.flatMap((s) => s.fields);
  const pwFld = flds.find((f) => f.kind === 'concealed' && /password/i.test(f.label));
  const pw = pwFld ? r.secrets[pwFld.id] : undefined;
  const out: T.FindingInfo[] = [];
  const push = (kind: string, description: string) => {
    if (!(d.ignoredFindings ?? []).some((k) => k === kind || (kind === 'missing_totp' && k === 'totp_available'))) out.push({ kind, description });
  };
  if (d.category === 'login' && pw && strength4(pw) < 2) push('weak', 'weak password');
  if (d.category === 'login' && pw) {
    const others = [...db.values()].filter((x) => {
      if (x.detail.id === d.id || x.detail.category !== 'login') return false;
      const f = x.detail.sections.flatMap((s) => s.fields).find((y) => y.kind === 'concealed' && /password/i.test(y.label));
      return f ? x.secrets[f.id] === pw : false;
    });
    if (others.length > 0) push('reused', `reused password (shared with ${others.length} other item(s))`);
  }
  if (d.category === 'login' && d.url && !d.hasTotp) push('missing_totp', 'has a URL but no TOTP configured');
  const u = d.url?.trim().toLowerCase();
  if (u?.startsWith('http://')) {
    try {
      const h = new URL(u).hostname;
      const local = h === 'localhost' || h.endsWith('.localhost') || h === 'local' || h.endsWith('.local') || h === '::1'
        || /^127\./.test(h) || /^10\./.test(h) || /^192\.168\./.test(h) || /^172\.(1[6-9]|2[0-9]|3[0-1])\./.test(h);
      if (!local) push('insecure_url', `saved with unencrypted HTTP URL (${h})`);
    } catch { /* malformed URL: no finding */ }
  }
  return out;
}
function genPw(length: number, symbols: boolean, digits: boolean, words?: boolean, pin?: boolean, separator?: string, capitalize?: boolean) {  const rnd = (n: number) => crypto.getRandomValues(new Uint32Array(1))[0] % n;
  if (pin) {
    const d = '0123456789';
    return Array.from({ length: Math.max(4, Math.min(16, length)) }, () => d[rnd(d.length)]).join('');
  }
  if (words) {
    const w = ['apple', 'river', 'stone', 'cloud', 'tiger', 'maple', 'ocean', 'pixel', 'lunar', 'cedar'];
    const count = Math.max(3, Math.min(20, length));
    const chosen = Array.from({ length: count }, () => {
      const word = w[rnd(w.length)];
      return capitalize ? word.charAt(0).toUpperCase() + word.slice(1) : word;
    });
    if (digits && chosen.length > 0) {
      const idx = rnd(chosen.length);
      chosen[idx] = chosen[idx] + rnd(10);
    }
    return chosen.join(separator ?? (symbols ? '-' : '.'));
  }
  const cs = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ' + (digits ? '23456789' : '') + (symbols ? '!@#$%^&*-_+=' : '');
  return Array.from({ length }, () => cs[rnd(cs.length)]).join('');
}

type A = any; // eslint-disable-line @typescript-eslint/no-explicit-any
let syncState: T.SyncStatus = { remote: 'git@example.com:demo/vault.git', lastSync: '2026-10-25T09:00:00Z', ahead: 0, behind: 1, dirty: false };
function resolveMockRelated(ids?: string[]): T.RelatedItem[] {
  if (!ids || ids.length === 0) return [];
  return ids.map((rid) => {
    const target = db.get(rid);
    if (!target || target.detail.deletedAt) {
      return { id: rid, title: "Item not available", available: false };
    }
    return {
      id: rid,
      vaultId: target.detail.vaultId,
      category: target.detail.category,
      title: target.detail.title,
      subtitle: target.detail.subtitle,
      available: true,
    };
  });
}

export const mock: Record<string, (a: A) => Promise<unknown>> = {
  status: () => delay<T.UnlockState>({ unlocked, vaultPath: '~/.gitpass (mock)', account: unlocked ? 'Demo Family' : null }),
  unlock: (a: { password: string }) => { if (!a.password) return Promise.reject(new Error('password required')); unlocked = true; return delay<T.UnlockState>({ unlocked, vaultPath: '~/.gitpass (mock)', account: 'Demo Family' }, 300); },
  lock: () => { unlocked = false; return delay(undefined); },
  show_quick_access: () => delay(undefined),
  create_account: (a: { password: string }) => { if ((a.password ?? '').length < 8) return Promise.reject(new Error('master password must be at least 8 characters')); unlocked = true; return delay<T.CreateAccountResult>({ vaultPath: '~/.gitpass (mock)', secretKey: 'A3-MOCK00-000000-00000-00000-00000-00000' }, 300); },
  unlock_biometric: () => Promise.reject(new Error('biometric unlock is not supported yet on this platform')),
  list_vaults: () => delay(vaultInfo()),
  update_vault: (a: { vaultId: string; name: string; description?: string | null; icon?: string | null; color?: string | null }) => {
    const v = vaults.find((x) => x.id === a.vaultId); if (!v) throw new Error('no vault');
    const name = a.name.trim(); if (!name) throw new Error('vault name is empty');
    if (vaults.some((x) => x.id !== a.vaultId && x.name === name)) throw new Error(`vault '${name}' already exists`);
    Object.assign(v, { name, description: a.description || null, icon: a.icon || null, color: a.color || null });
    return delay(vaultInfo().find((x) => x.id === v.id)!);
  },
  delete_vault: (a: { vaultId: string; confirmName: string }) => {
    const i = vaults.findIndex((x) => x.id === a.vaultId); if (i < 0) throw new Error('no vault');
    if (vaults.length <= 1) throw new Error("can't delete the last vault");
    if (vaults[i].name !== a.confirmName) throw new Error('confirmation name does not match');
    vaults.splice(i, 1); for (const [k, r] of db) if (r.detail.vaultId === a.vaultId) db.delete(k);
    return delay(undefined);
  },
  create_vault: (a: { name: string; description?: string | null; icon?: string | null; color?: string | null }) => {
    const v: T.VaultInfo = { id: uid('v'), name: a.name, description: a.description ?? null, itemCount: 0, shared: false, members: [SELF], icon: a.icon ?? null, color: a.color ?? null };
    vaults.push(v); return delay(v);
  },
  list_items: ({ filter }: { filter: T.Filter }) => {
    let xs = [...db.values()].map((r) => r.detail);
    switch (filter.kind) {
      case 'deleted': xs = xs.filter((x) => x.deletedAt); break;
      case 'archive': xs = xs.filter((x) => x.archived && !x.deletedAt); break;
      default: xs = xs.filter((x) => !x.archived && !x.deletedAt);
        if (filter.kind === 'favorites') xs = xs.filter((x) => x.favorite);
        if (filter.kind === 'vault') xs = xs.filter((x) => x.vaultId === filter.vaultId);
        if (filter.kind === 'tag') xs = xs.filter((x) => x.tags.includes(filter.tag ?? ''));
    }
    if (filter.category) xs = xs.filter((x) => x.category === filter.category);
    if (filter.query) {
      const tokens = filter.query.match(/(?:[^\s"']+|"[^"]*"|'[^']*')+/g) ?? [];
      const terms: string[] = [];
      for (const tok of tokens) {
        const negated = tok.startsWith('-');
        const body = negated ? tok.slice(1) : tok;
        const idx = body.indexOf(':');
        if (idx !== -1) {
          const op = body.slice(0, idx).toLowerCase();
          let val = body.slice(idx + 1);
          if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
            val = val.slice(1, -1);
          }
          val = val.trim();
          if (op === 'tag' && val) {
            xs = xs.filter((x) => {
              const m = x.tags.some((t) => t.toLowerCase() === val.toLowerCase());
              return negated ? !m : m;
            });
            continue;
          }
          if (op === 'vault' && val) {
            const vname = (id: string) => vaults.find((v) => v.id === id)?.name.toLowerCase() ?? '';
            xs = xs.filter((x) => {
              const m = vname(x.vaultId).startsWith(val.toLowerCase());
              return negated ? !m : m;
            });
            continue;
          }
          if (op === 'type' && val) {
            const catMap: Record<string, string> = { card: 'credit_card', note: 'secure_note', api: 'api_credential', ssh: 'ssh_key', bank: 'bank_account' };
            const wantCat = catMap[val.toLowerCase()] ?? val.toLowerCase();
            xs = xs.filter((x) => {
              const m = x.category.toLowerCase() === wantCat;
              return negated ? !m : m;
            });
            continue;
          }
          if (op === 'is') {
            const vLow = val.toLowerCase();
            if (vLow === 'favorite' || vLow === 'fav') {
              xs = xs.filter((x) => (negated ? !x.favorite : x.favorite));
              continue;
            }
            if (vLow === 'archived') {
              xs = xs.filter((x) => (negated ? !x.archived : x.archived));
              continue;
            }
          }
          if (op === 'has') {
            const vLow = val.toLowerCase();
            if (vLow === 'otp' || vLow === 'totp') {
              xs = xs.filter((x) => (negated ? !x.hasTotp : x.hasTotp));
              continue;
            }
            if (vLow === 'passkey') {
              xs = xs.filter((x) => (negated ? x.category !== 'passkey' : x.category === 'passkey'));
              continue;
            }
            if (vLow === 'attachment' || vLow === 'attachments' || vLow === 'file' || vLow === 'files') {
              xs = xs.filter((x) => (negated ? x.attachments.length === 0 : x.attachments.length > 0));
              continue;
            }
            if (vLow === 'url' || vLow === 'website') {
              xs = xs.filter((x) => (negated ? !x.url : Boolean(x.url)));
              continue;
            }
          }
          if (op === 'url' && val) {
            xs = xs.filter((x) => {
              const u = (x.url ?? '').toLowerCase();
              const host = u.replace(/^[a-z]+:\/\//, '').split(/[/?#]/)[0].replace(/^www\./, '');
              const target = val.toLowerCase().replace(/^[a-z]+:\/\//, '').split(/[/?#]/)[0].replace(/^www\./, '');
              const m = host === target || host.endsWith(`.${target}`) || host.includes(target);
              return negated ? !m : m;
            });
            continue;
          }
        }
        terms.push(tok);
      }
      if (terms.length > 0) {
        const q = terms.join(' ').toLowerCase();
        xs = xs.filter((x) => `${x.title} ${x.subtitle} ${x.url ?? ''} ${x.tags.join(' ')}`.toLowerCase().includes(q));
      }
    }
    const sort = filter.sort ?? 'updated';
    xs.sort((a, b) => sort === 'title' ? a.title.localeCompare(b.title) : sort === 'created' ? b.createdAt.localeCompare(a.createdAt) : sort === 'recent' ? (b.lastUsed ?? 0) - (a.lastUsed ?? 0) || a.title.localeCompare(b.title) : b.updatedAt.localeCompare(a.updatedAt));
    return delay(xs.map(summary));
  },
  get_item: (a: { itemId: string }) => {
    const d = get(a.itemId).detail;
    return delay({ ...d, relatedItems: resolveMockRelated(d.relatedItemIds), findings: mockFindings(a.itemId) });
  },
  reveal_field: (a: { itemId: string; fieldId: string; version?: number | null }) => {
    const r = get(a.itemId);
    r.detail.lastUsed = Math.floor(Date.now() / 1000);
    const v = r.secrets[a.fieldId] ?? r.detail.sections.flatMap((s) => s.fields).find((x) => x.id === a.fieldId)?.value;
    return v == null ? Promise.reject(new Error('not a concealed field')) : delay(v);
  },
  copy_field: (a: { itemId: string; fieldId: string }) => {
    const r = get(a.itemId);
    r.detail.lastUsed = Math.floor(Date.now() / 1000);
    const fld = r.detail.sections.flatMap((s) => s.fields).find((x) => x.id === a.fieldId);
    const v = r.secrets[a.fieldId] ?? fld?.value ?? '';
    // In Tauri the copy happens in Rust; the browser mock best-effort uses the clipboard API.
    navigator.clipboard?.writeText(v).catch(() => {});
    return delay(undefined);
  },
  totp_now: () => { const period = 30; const now = Math.floor(Date.now() / 1000); const code = String((Math.floor(now / period) * 7919) % 1000000).padStart(6, '0'); return delay({ code, remaining: period - (now % period), period }, 10); },
  scan_totp: (_a: { source: 'screen' | 'clipboard' | 'image'; dataBase64?: string }) => {
    return delay<T.ScannedTotp>({
      uri: 'otpauth://totp/GitHub:octocat?secret=JBSWY3DPEHPK3PXP&issuer=GitHub&algorithm=SHA1&digits=6&period=30',
      secret: 'JBSWY3DPEHPK3PXP',
      issuer: 'GitHub',
      account: 'octocat',
      digits: 6,
      period: 30,
      algorithm: 'SHA1',
    }, 20);
  },
  create_item: ({ input }: { input: T.ItemInput }) => {
    const now = new Date().toISOString();
    const r: Rec = { secrets: {}, versions: [], detail: { id: uid('i'), vaultId: input.vaultId, category: input.category, title: input.title, subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: now, createdAt: now, url: null, sections: [], notes: null, attachments: [], history: [{ version: 1, updatedAt: now }], passwordStrength: null, hasTotp: false, passwordChangedAt: null } };
    applyInput(r, input); db.set(r.detail.id, r); return delay(summary(r.detail));
  },
  update_item: ({ itemId, input }: { itemId: string; input: T.ItemInput }) => { const r = get(itemId); touch(r); applyInput(r, input); return delay(summary(r.detail)); },
  set_favorite: (a: { itemId: string; favorite: boolean }) => { get(a.itemId).detail.favorite = a.favorite; return delay(undefined); },
  archive_item: (a: { itemId: string; archived: boolean }) => { get(a.itemId).detail.archived = a.archived; return delay(undefined); },
  delete_item: (a: { itemId: string }) => { get(a.itemId).detail.deletedAt = new Date().toISOString(); return delay(undefined); },
  restore_item: (a: { itemId: string }) => { const d = get(a.itemId).detail; d.deletedAt = null; d.archived = false; return delay(undefined); },
  purge_item: (a: { itemId: string }) => { db.delete(a.itemId); return delay(undefined); },
  empty_trash: () => { let n = 0; for (const [id, r] of db) if (r.detail.deletedAt) { db.delete(id); n++; } return delay(n); },
  bulk_items: ({ req }: { req: T.BulkRequest }) => {
    const now = new Date().toISOString(); const out: T.ItemRef[] = []; let count = 0;
    const tag = (req.tag ?? '').trim();
    if ((req.action === 'add_tag' || req.action === 'remove_tag') && !tag && req.items.length) return Promise.reject(new Error('tag is required'));
    if ((req.action === 'move' || req.action === 'copy') && req.items.length && !vaults.some((v) => v.id === req.toVaultId)) return Promise.reject(new Error('no vault'));
    for (const ref of [...new Map(req.items.map((x) => [x.itemId, x])).values()]) {
      const r = db.get(ref.itemId); if (!r) return Promise.reject(new Error(`no item ${ref.itemId}`));
      const d = r.detail; count++;
      switch (req.action) {
        case 'trash': d.deletedAt ??= now; break;
        case 'restore': d.deletedAt = null; break;
        case 'archive': d.archived = true; break;
        case 'unarchive': d.archived = false; break;
        case 'favorite': d.favorite = true; break;
        case 'unfavorite': d.favorite = false; break;
        case 'add_tag': if (!d.tags.includes(tag)) d.tags.push(tag); break;
        case 'remove_tag': d.tags = d.tags.filter((t) => t !== tag); break;
        case 'purge': db.delete(ref.itemId); continue;
        case 'move': if (d.vaultId === req.toVaultId) { count--; continue; } d.vaultId = req.toVaultId!; touch(r); break;
        case 'copy': { const c: Rec = structuredClone(r); Object.assign(c.detail, { id: uid('i'), vaultId: req.toVaultId!, createdAt: now, updatedAt: now }); c.versions = []; db.set(c.detail.id, c); out.push({ vaultId: c.detail.vaultId, itemId: c.detail.id }); continue; }
        default: return Promise.reject(new Error(`unknown bulk action '${req.action}'`));
      }
      out.push({ vaultId: d.vaultId, itemId: d.id });
    }
    return delay({ count, items: out });
  },
  move_item: (a: { itemId: string; toVaultId: string }) => { const r = get(a.itemId); touch(r); r.detail.vaultId = a.toVaultId; return delay(summary(r.detail)); },
  duplicate_item: (a: { itemId: string }) => {
    const src = get(a.itemId); const r: Rec = structuredClone(src); const now = new Date().toISOString();
    r.versions = []; Object.assign(r.detail, { id: uid('i'), title: `${src.detail.title} (copy)`, createdAt: now, updatedAt: now, history: [{ version: 1, updatedAt: now }] });
    db.set(r.detail.id, r); return delay(summary(r.detail));
  },
  item_version: (a: { itemId: string; version: number }) => {
    const r = get(a.itemId);
    const snap = r.versions.find((v) => (v as any).version === a.version) ?? (r.detail.version === a.version ? r.detail : r.versions[a.version - 1] ?? r.detail);
    const copy = structuredClone(snap);
    copy.history = [];
    copy.attachments = [];
    annotateMockDiff(copy, r.detail);
    return delay(copy);
  },
  restore_item_version: (a: { vaultId: string; itemId: string; version: number }) => {
    const r = get(a.itemId);
    if (r.detail.deletedAt) return Promise.reject(new Error('cannot restore version of deleted item'));
    const target = r.versions.find((v) => (v as any).version === a.version) ?? (r.detail.version === a.version ? r.detail : r.versions[a.version - 1]);
    if (!target) return Promise.reject(new Error(`no version ${a.version} in item history`));
    touch(r);
    r.detail.title = target.title;
    r.detail.category = target.category;
    r.detail.sections = structuredClone(target.sections);
    r.detail.notes = target.notes;
    r.detail.tags = [...target.tags];
    r.detail.url = target.url;
    r.detail.subtitle = target.subtitle;
    r.detail.hasTotp = target.hasTotp;
    r.detail.passwordStrength = target.passwordStrength;
    return delay(r.detail);
  },
  add_attachment: (a: { itemId: string; path: string }) => { const att: T.Attachment = { id: uid('a'), name: a.path.split(/[\\/]/).pop() || 'file', size: 4096, mime: 'application/octet-stream' }; get(a.itemId).detail.attachments.push(att); return delay(att); },
  save_attachment: () => delay(undefined),
  remove_attachment: (a: { itemId: string; attachmentId: string }) => {
    const item = get(a.itemId);
    const idx = item.detail.attachments.findIndex((x) => x.id === a.attachmentId);
    if (idx < 0) return Promise.reject(new Error(`item has no attachment ${a.attachmentId}`));
    item.detail.attachments.splice(idx, 1);
    return delay(undefined);
  },
  list_tags: () => delay([...new Set([...db.values()].flatMap((r) => r.detail.tags))].sort()),
  rename_tag: (a: { old: string; new: string }) => {
    const old = a.old.trim();
    const next = a.new.trim();
    if (!old) return Promise.reject(new Error('tag name cannot be empty'));
    if (!next) return Promise.reject(new Error('new tag name cannot be empty'));
    if (old === next) return delay({ count: 0, items: [] });
    const items: T.ItemRef[] = [];
    for (const r of db.values()) {
      if (r.detail.tags.includes(old)) {
        const set = new Set<string>();
        for (const t of r.detail.tags) {
          set.add(t === old ? next : t);
        }
        r.detail.tags = [...set];
        items.push({ vaultId: r.detail.vaultId, itemId: r.detail.id });
      }
    }
    return delay({ count: items.length, items });
  },
  delete_tag: (a: { name: string }) => {
    const name = a.name.trim();
    if (!name) return Promise.reject(new Error('tag name cannot be empty'));
    const items: T.ItemRef[] = [];
    for (const r of db.values()) {
      if (r.detail.tags.includes(name)) {
        r.detail.tags = r.detail.tags.filter((t) => t !== name);
        items.push({ vaultId: r.detail.vaultId, itemId: r.detail.id });
      }
    }
    return delay({ count: items.length, items });
  },
  generate_password: (a: { length: number; symbols: boolean; digits: boolean; words?: boolean; pin?: boolean; separator?: string; capitalize?: boolean }) => delay(genPw(a.length, a.symbols, a.digits, a.words, a.pin, a.separator, a.capitalize), 5),
  copy_password: (a: { password: string }) => {
    navigator.clipboard?.writeText(a.password).catch(() => {});
    return delay(undefined);
  },
  password_strength: (a: { password: string }) => delay(strength(a.password), 5),
  ignore_finding: (a: { vaultId: string; itemId: string; kind: string }) => {
    const r = db.get(a.itemId);
    if (r) {
      const cur = r.detail.ignoredFindings ?? [];
      if (!cur.includes(a.kind)) r.detail.ignoredFindings = [...cur, a.kind];
    }
    return delay(undefined, 20);
  },
  unignore_finding: (a: { vaultId: string; itemId: string; kind: string }) => {
    const r = db.get(a.itemId);
    if (r) {
      r.detail.ignoredFindings = (r.detail.ignoredFindings ?? []).filter((k) => k !== a.kind);
    }
    return delay(undefined, 20);
  },
  add_related_item: (a: { vaultId: string; itemId: string; relatedId: string }) => {
    if (a.itemId === a.relatedId) return Promise.reject(new Error("cannot link an item to itself"));
    const r = db.get(a.itemId);
    if (r) {
      const cur = r.detail.relatedItemIds ?? [];
      if (!cur.includes(a.relatedId)) r.detail.relatedItemIds = [...cur, a.relatedId];
    }
    return delay(undefined, 20);
  },
  remove_related_item: (a: { vaultId: string; itemId: string; relatedId: string }) => {
    const r = db.get(a.itemId);
    if (r) {
      r.detail.relatedItemIds = (r.detail.relatedItemIds ?? []).filter((id) => id !== a.relatedId);
    }
    return delay(undefined, 20);
  },
  watchtower: (a: { hibp: boolean }) => {
    const pw = [...db.values()].map((r) => { const fld = r.detail.sections.flatMap((s) => s.fields).find((x) => x.kind === 'concealed' && /password/.test(x.label)); return fld ? [r.detail.id, r.secrets[fld.id]] as const : null; }).filter(Boolean) as (readonly [string, string])[];
    const hibp = a.hibp && settings.hibpCheck; // like Rust: network check only when opted in
    const weak = pw.filter(([, p]) => strength4(p) < 2).map(([id]) => id);
    const groups = new Map<string, string[]>(); pw.forEach(([id, p]) => groups.set(p, [...(groups.get(p) ?? []), id]));
    const reused = [...groups.values()].filter((g) => g.length > 1);
    const logins = [...db.values()].filter((r) => r.detail.category === 'login');
    const missingTotp = logins.filter((r) => !r.detail.hasTotp).map((r) => r.detail.id);
    // Demo analogue of the core offline site list: GitHub is TOTP+passkey, banking is passkey-capable.
    const totpAvailable = logins.filter((r) => !r.detail.hasTotp && /github/i.test(r.detail.title)).map((r) => r.detail.id);
    const passkeyAvailable = logins.filter((r) => /github|bank/i.test(r.detail.title)).map((r) => r.detail.id);
    const old = logins.filter((r) => Date.parse(r.detail.updatedAt) < Date.parse('2026-09-01')).map((r) => r.detail.id);
    const now = Date.now();
    const expired: string[] = [];
    const expiringSoon: string[] = [];
    for (const r of db.values()) {
      if (r.detail.archived) continue;
      for (const s of r.detail.sections) {
        for (const fl of s.fields) {
          if (!fl.value || (!fl.id.toLowerCase().includes('expir') && !fl.label.toLowerCase().includes('expir'))) continue;
          const v = fl.value.trim();
          let expTs: number | null = null;
          const m = /^\s*(\d{1,2})\s*[/\-. ]?\s*(\d{2}|\d{4})\s*$/.exec(v);
          if (m) {
            const mm = Number(m[1]);
            let yy = Number(m[2]);
            if (yy < 100) yy += 2000;
            if (mm >= 1 && mm <= 12) {
              const nextM = mm === 12 ? 1 : mm + 1;
              const nextY = mm === 12 ? yy + 1 : yy;
              expTs = Date.UTC(nextY, nextM - 1, 1) - 1000;
            }
          } else {
            const ym = /^\s*(\d{4})\s*[/\-. ]\s*(\d{1,2})\s*$/.exec(v);
            if (ym) {
              const yy = Number(ym[1]);
              const mm = Number(ym[2]);
              if (mm >= 1 && mm <= 12) {
                const nextM = mm === 12 ? 1 : mm + 1;
                const nextY = mm === 12 ? yy + 1 : yy;
                expTs = Date.UTC(nextY, nextM - 1, 1) - 1000;
              }
            } else {
              const d = Date.parse(v);
              if (!isNaN(d)) expTs = d;
            }
          }
          if (expTs !== null) {
            if (now > expTs) expired.push(r.detail.id);
            else if ((expTs - now) / 86400000 <= 30) expiringSoon.push(r.detail.id);
          }
        }
      }
    }
    const insecureUrls = logins.filter((r) => {
      const u = r.detail.url?.trim().toLowerCase();
      if (!u || !u.startsWith('http://')) return false;
      try {
        const parsed = new URL(u);
        const h = parsed.hostname;
        if (h === 'localhost' || h.endsWith('.localhost') || h === 'local' || h.endsWith('.local') || h === '::1') return false;
        if (h.startsWith('127.')) return false;
        if (h.startsWith('10.') || h.startsWith('192.168.')) return false;
        if (/^172\.(1[6-9]|2[0-9]|3[0-1])\./.test(h)) return false;
        return true;
      } catch {
        return false;
      }
    }).map((r) => r.detail.id);
    const dupMap = new Map<string, string[]>();
    for (const r of logins) {
      if (r.detail.archived || r.detail.deletedAt) continue;
      const user = r.detail.subtitle?.trim().toLowerCase();
      const rawUrl = r.detail.url?.trim() || '';
      const host = rawUrl.replace(/^[a-zA-Z]+:\/\//, '').split(/[/?:#]/)[0].split('@').pop()?.split(':')[0].toLowerCase().replace(/^www\./, '');
      if (user && host) {
        const k = `${r.detail.vaultId}|${host}|${user}`;
        dupMap.set(k, [...(dupMap.get(k) ?? []), r.detail.id]);
      }
    }
    const duplicates = [...dupMap.values()].filter((g) => g.length > 1);
    // Demo of the unused-items check: logins not edited for the configured threshold (never-used
    // items fall back to their edit time, like the Rust audit).
    const unusedMonths = settings.watchtowerUnusedMonths ?? 12;
    const unusedPool: T.UnusedItem[] = unusedMonths > 0
      ? logins
          .filter((r) => !r.detail.archived && !r.detail.deletedAt)
          .map((r) => ({ id: r.detail.id, edited: Date.parse(r.detail.updatedAt) }))
          .filter((x) => Number.isFinite(x.edited) && Date.now() - x.edited > (unusedMonths * 365 * 86400000) / 12)
          .map((x) => ({ itemId: x.id, days: Math.floor((Date.now() - x.edited) / 86400000), lastUsed: false }))
      : [];

    const isIgnored = (id: string, kind: string) => {
      const it = db.get(id);
      return (it?.detail.ignoredFindings ?? []).some(
        (k) =>
          k === kind ||
          (kind === "missing_totp" && k === "totp_available") ||
          (kind === "totp_available" && k === "missing_totp") ||
          (kind === "pwned" && k === "breached") ||
          (kind === "breached" && k === "pwned")
      );
    };

    const ignoredList: T.IgnoredFinding[] = [];
    const checkOrIgnore = (id: string, kind: string, desc: string, activeList: string[]) => {
      if (isIgnored(id, kind)) {
        const item = db.get(id)?.detail;
        ignoredList.push({
          itemId: id,
          vaultId: item?.vaultId ?? "",
          title: item?.title ?? id,
          kind,
          description: desc,
        });
      } else {
        activeList.push(id);
      }
    };

    const activeWeak: string[] = [];
    for (const id of weak) checkOrIgnore(id, "weak", "Weak password", activeWeak);

    const activeReused: string[][] = [];
    for (const g of reused) {
      const filtered: string[] = [];
      for (const id of g) {
        if (isIgnored(id, "reused")) {
          const item = db.get(id)?.detail;
          ignoredList.push({
            itemId: id,
            vaultId: item?.vaultId ?? "",
            title: item?.title ?? id,
            kind: "reused",
            description: "Reused password",
          });
        } else {
          filtered.push(id);
        }
      }
      if (filtered.length > 1) activeReused.push(filtered);
    }

    const activeOld: string[] = [];
    for (const id of old) checkOrIgnore(id, "old", "Old password", activeOld);

    const activeMissingTotp: string[] = [];
    for (const id of missingTotp) checkOrIgnore(id, "missing_totp", "Missing two-factor authentication", activeMissingTotp);

    const activeTotpAvailable: string[] = [];
    for (const id of totpAvailable) checkOrIgnore(id, "totp_available", "Two-factor authentication available", activeTotpAvailable);

    const activePasskeyAvailable: string[] = [];
    for (const id of passkeyAvailable) checkOrIgnore(id, "passkey_available", "Passkey available", activePasskeyAvailable);

    const activeExpired: string[] = [];
    for (const id of expired) checkOrIgnore(id, "expired", "Expired", activeExpired);

    const activeExpiringSoon: string[] = [];
    for (const id of expiringSoon) checkOrIgnore(id, "expiring_soon", "Expiring soon", activeExpiringSoon);

    const activeInsecureUrls: string[] = [];
    for (const id of insecureUrls) checkOrIgnore(id, "insecure_url", "Saved with unencrypted HTTP URL", activeInsecureUrls);

    const activeUnusedIds: string[] = [];
    for (const u of unusedPool) checkOrIgnore(u.itemId, "unused", "Not used in a long time", activeUnusedIds);
    const activeUnused = unusedPool.filter((u) => activeUnusedIds.includes(u.itemId));

    const activeBreached: string[] = [];
    if (hibp && activeWeak.length > 0) {
      const candidate = activeWeak[0];
      checkOrIgnore(candidate, "pwned", "Breached password (HIBP)", activeBreached);
    }

    return delay<T.WatchtowerReport>({
      score: Math.max(0, 100 - activeWeak.length * 6 - activeReused.length * 8 - activeOld.length * 2),
      weak: activeWeak,
      reused: activeReused,
      old: activeOld,
      breached: activeBreached,
      missingTotp: activeMissingTotp,
      totpAvailable: activeTotpAvailable,
      passkeyAvailable: activePasskeyAvailable,
      expired: activeExpired,
      expiringSoon: activeExpiringSoon,
      insecureUrls: activeInsecureUrls,
      duplicates,
      weakSshKeys: [],
      unused: activeUnused,
      weakReasons: Object.fromEntries(activeWeak.map((id) => [id, "contains the username"])),
      ignored: ignoredList,
      checkedAt: new Date().toISOString(),
      hibpChecked: hibp,
    }, 200);
  },
  import_file: (a: { dryRun: boolean }) => { if (!a.dryRun) onboarding = { ...onboarding, imported: true }; return delay<T.ImportResult>({ parsed: 12, added: a.dryRun ? 0 : 10, duplicates: 2, duplicateTitles: ['Example Mail', 'Example Mail'], warnings: ['row 4: skipped empty row'], categories: { login: 10, credit_card: 1, secure_note: 1 },
    preview: [{ title: 'Example Mail', category: 'login', username: 'alice@example.invalid', url: 'https://mail.example.invalid', hasPassword: true, hasTotp: true, tags: ['Work'], fields: 4, archived: false },
      { title: 'Fake Visa', category: 'credit_card', username: null, url: null, hasPassword: false, hasTotp: false, tags: [], fields: 5, archived: false }] }, 300); },
  export_items: (a: { req: T.ExportRequest }) => delay<T.ExportResult>({ path: a.req.path, items: a.req.items?.length ?? [...db.values()].length, format: a.req.format, encrypted: a.req.encrypted }, 250),
  ssh_generate: (a: { vaultId: string; title: string; comment?: string | null }) => {
    const n = Math.floor(Math.random() * 1e6); const pub = `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKE${n} ${a.comment || a.title}`; const fp = `SHA256:FAKE${n}`;
    return (mock.create_item({ input: { vaultId: a.vaultId, category: 'ssh_key', title: a.title || 'SSH Key', sections: [{ id: 'main', title: null, fields: [
      { id: 'private_key', label: 'private key', kind: 'concealed', value: '-----BEGIN OPENSSH PRIVATE KEY-----\nFAKE\n-----END OPENSSH PRIVATE KEY-----' },
      { id: 'public_key', label: 'public key', kind: 'multiline', value: pub }, { id: 'fingerprint', label: 'fingerprint', kind: 'text', value: fp }, { id: 'key_type', label: 'key type', kind: 'text', value: 'ssh-ed25519' }] }] } }) as Promise<T.ItemSummary>)
      .then((item) => ({ item, publicKey: pub, fingerprint: fp, keyType: 'ssh-ed25519' }));
  },
  ssh_import: (a: { vaultId: string; title: string; privateKey: string; passphrase?: string | null }) => {
    if (!/-----BEGIN OPENSSH PRIVATE KEY-----/.test(a.privateKey)) return Promise.reject(new Error('not an OpenSSH private key'));
    return mock.ssh_generate({ vaultId: a.vaultId, title: a.title || 'Imported key', comment: 'imported' });
  },
  ssh_agent_info: () => delay<T.SshAgentInfo>({ socketPath: '/Users/demo/Library/Application Support/gitpass/ssh-agent.sock', configLine: 'Host *\n  IdentityAgent "~/Library/Application Support/gitpass/ssh-agent.sock"', running: true, approveEachUse: !!settings.sshApproveEachUse }),
  ssh_agent_activity: () => delay<T.SshActivityEntry[]>(mockSshActivity),
  clear_ssh_agent_activity: () => { mockSshActivity = []; return delay<void>(undefined); },
  ssh_git_signing_config: (a: { vaultId: string; itemId: string; email?: string | null }) => {
    const rec = db.get(a.itemId);
    if (!rec) return Promise.reject(new Error('item not found'));
    const item = rec.detail;
    if (item.category !== 'ssh_key') return Promise.reject(new Error('item is not an SSH key'));
    const pubField = item.sections.flatMap((s: T.Section) => s.fields).find((f: T.Field) => f.id === 'public_key');
    const pub = pubField?.value || 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5FAKEDEMO demo@example.com';
    const typeField = item.sections.flatMap((s: T.Section) => s.fields).find((f: T.Field) => f.id === 'key_type');
    const keyType = typeField?.value || (pub.startsWith('ssh-rsa') ? 'ssh-rsa' : pub.startsWith('ecdsa') ? 'ecdsa-sha2-nistp256' : 'ssh-ed25519');
    if (keyType !== 'ssh-ed25519' && keyType !== 'ssh-rsa') {
      return Promise.reject(new Error(`Git commit signing supports Ed25519 and RSA keys; found ${keyType}`));
    }
    const email = a.email || 'demo@example.com';
    const allowedSignersFile = '~/.ssh/allowed_signers';
    const socketPath = '/Users/demo/Library/Application Support/gitpass/ssh-agent.sock';
    const exportAuthSock = `export SSH_AUTH_SOCK="${socketPath}"`;
    const signingKey = `key::${pub.trim()}`;
    const allowedSignersLine = `${email} ${pub.trim()}`;
    const configLines = `gpg.format=ssh\nuser.signingkey="${signingKey}"\ncommit.gpgsign=true\ntag.gpgsign=true\ngpg.ssh.allowedSignersFile=${allowedSignersFile}`;
    return delay<T.GitSigningConfig>({
      keyType,
      publicKey: pub.trim(),
      signingKey,
      socketPath,
      exportAuthSock,
      allowedSignersFile,
      allowedSignersLine,
      configLines,
      email,
      authSockActive: false,
    });
  },
  ssh_apply_git_signing: (a: { vaultId: string; itemId: string; email?: string | null; allowedSignersFile?: string | null; global?: boolean }) => {
    return mock.ssh_git_signing_config({ vaultId: a.vaultId, itemId: a.itemId, email: a.email });
  },
  identity: () => delay<T.Identity>({ publicKey: SELF_KEY, fingerprint: SELF.fingerprint }),
  fingerprint_of: (a: { publicKey: string }) => (PUB_RE.test(a.publicKey.trim()) ? delay(mockFingerprint(a.publicKey), 20) : Promise.reject(new Error('public key is not valid base64'))),
  list_members: (a: { vaultId: string }) => delay(vaults.find((v) => v.id === a.vaultId)?.members ?? []),
  share_vault: (a: { vaultId: string; publicKey: string; label?: string | null }) => {
    const v = vaults.find((x) => x.id === a.vaultId); if (!v) return Promise.reject(new Error('vault not found'));
    if (!PUB_RE.test(a.publicKey.trim())) return Promise.reject(new Error('public key is not valid base64'));
    const m: T.Member = { fingerprint: mockFingerprint(a.publicKey), label: a.label ?? null, isSelf: false }; v.members.push(m); v.shared = true; return delay(m);
  },
  unshare_vault: (a: { vaultId: string; fingerprint: string }) => { const v = vaults.find((x) => x.id === a.vaultId); if (v) { v.members = v.members.filter((m) => m.fingerprint !== a.fingerprint); v.shared = v.members.length > 1; } return delay(undefined); },
  accept_share: () => { const v: T.VaultInfo = { id: uid('v'), name: 'Accepted Share (demo)', description: null, itemCount: 0, shared: true, members: [SELF], icon: null, color: '#22c55e' }; vaults.push(v); return delay(v); },
  list_devices: () => delay(devices),
  revoke_device: (a: { id: string }) => { const d = devices.find((x) => x.id === a.id); if (d) d.revoked = true; return delay(undefined); },
  get_settings: () => delay(settings),
  set_settings: (a: { settings: T.Settings }) => { settings = { ...a.settings }; return delay(settings); },
  change_password: (a: { current: string; next: string }) => (!a.current || !a.next ? Promise.reject(new Error('both passwords required')) : delay(undefined, 300)),
  rotate_secret_key: () => delay({ emergencyKit: 'gitpass Emergency Kit (DEMO)\nSecret Key: A3-FAKE00-FAKE00-FAKE00-FAKE00' }),
  sync_status: () => delay<T.SyncStatus>({ ...syncState }),
  remote_connect: (a: { url: string }) => (/fail|bad/.test(a.url) ? Promise.reject(new Error('AUTH_FAILED: git push failed (mock)'))
    : (syncState = { remote: a.url, lastSync: new Date().toISOString(), ahead: 0, behind: 0, dirty: false }, onboarding = { ...onboarding, repoConnected: true }, delay<T.SyncStatus>({ ...syncState }, 400))),
  remote_disconnect: () => { syncState = { remote: null, lastSync: syncState.lastSync, ahead: 0, behind: 0, dirty: false }; onboarding = { ...onboarding, repoConnected: false }; return delay<T.SyncStatus>({ ...syncState }); },
  open_new_repo_page: () => { if (typeof window !== 'undefined') window.open('https://github.com/new', '_blank', 'noopener'); return delay(undefined); },
  open_github_ssh_keys_page: () => { if (typeof window !== 'undefined') window.open('https://github.com/settings/ssh/new', '_blank', 'noopener'); return delay(undefined); },
  open_change_password: (a: { itemId: string }) => {
    const url = summary(get(a.itemId).detail).changePasswordUrl;
    if (!url) return Promise.reject(new Error('this item has no change-password URL'));
    if (typeof window !== 'undefined') window.open(url, '_blank', 'noopener');
    return delay(undefined);
  },
  // Dev mock only: the real path derives https://<rp_id> in Rust (open_passkey_site).
  open_passkey_site: (a: { itemId: string }) => {
    const d = get(a.itemId).detail;
    const site = d.sections.flatMap((s) => s.fields).find((f) => /^(site|website|rp_id)$/i.test(f.label) || f.label === 'rp_id');
    const host = (site?.value ?? '') || (() => { try { return new URL(d.url ?? '').hostname; } catch { return ''; } })();
    if (!host) return Promise.reject(new Error('this item has no passkey site'));
    if (typeof window !== 'undefined') window.open(`https://${host}`, '_blank', 'noopener');
    return delay(undefined);
  },
  sync: () => delay({ message: 'Pulled 1 commit (mock)' }, 400),
  onboarding: () => delay(onboarding),
  set_onboarding: (a: { patch: Partial<T.Onboarding> }) => { onboarding = { ...onboarding, ...a.patch }; return delay(onboarding); },
  emergency_kit_data: (a: { password?: string }) => (a?.password !== undefined && a.password !== 'demo-password' ? Promise.reject(new Error('wrong master password'))
    : delay<T.EmergencyKitData>({ accountId: 'acc-demo-0000', vaultPath: '~/.gitpass (mock)', remote: 'git@example.com:demo/vault.git', deviceName: 'Demo Mac', createdAt: '2026-10-26T09:00:00Z', secretKeyAvailable: true, secretKey: a?.password ? 'A3-MOCK00-000000-00000-00000-00000-00000' : null })),
  emergency_kit: () => delay('gitpass Emergency Kit (DEMO)\nAccount: Demo Family\nSecret Key: (not shown — rotate to reveal a new one)'),
  favicon_url: (a?: { url?: string }) => delay(settings.showWebsiteIcons && a?.url?.startsWith('https://') ? 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PGNpcmNsZSBjeD0iMTIiIGN5PSIxMiiciHI9IjEwIiBmaWxsPSIjZjI4YzI4Ii8+PC9zdmc+' : null),
  get_generator_history: () => delay([...mockGeneratorHistory]),
  delete_generator_history_entry: (a: { id: string }) => {
    mockGeneratorHistory = mockGeneratorHistory.filter((e) => e.id !== a.id);
    return delay(true);
  },
  clear_generator_history: () => {
    mockGeneratorHistory = [];
    return delay(undefined);
  },
  record_generated_password: (a: { password: string; source: string; host?: string | null }) => {
    if (settings.rememberGeneratedPasswords === false) return delay(null);
    const e: T.GeneratorHistoryEntry = {
      id: "gh-" + Math.floor(Math.random() * 1e6),
      value: a.password,
      createdAt: Math.floor(Date.now() / 1000),
      source: a.source,
      host: a.host ?? null,
    };
    mockGeneratorHistory = [e, ...mockGeneratorHistory].slice(0, 100);
    return delay(e);
  },
};
