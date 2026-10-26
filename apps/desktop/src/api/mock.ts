// Canonical in-browser mock for `npm run dev` outside Tauri. ALL DATA IS FAKE.
// Concealed values are kept in a private map and only returned by reveal_field,
// mirroring the Rust security model (detail payloads carry value: null).
import type * as T from './types';

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
  return [{ id, label, kind, value: hidden ? null : value }, hidden ? value : null];
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
    updatedAt: ts, createdAt: new Date(Date.parse(ts) - 30 * 86400000).toISOString(), url: null,
    sections: [{ id: uid('s'), title: null, fields: fs }], notes: null, attachments: [],
    history: [{ version: 1, updatedAt: ts }], passwordStrength: pwField ? strength(secrets[pwField.id]) : null,
    hasTotp: fs.some((x) => x.kind === 'totp'), ...extra,
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
  ['Placeholder Forum', 'lurker', 'https://forum.example.io', 'qwerty', 34],
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
cards.forEach(([t, n, d], i) => add(i % 2 ? 'v_family' : 'v_private', 'credit_card', t, `${n.slice(0, 4)} **** ${n.slice(-4)}`, d, [
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
{ const a = [...db.values()][5].detail; a.archived = true; const b = [...db.values()][22].detail; b.deletedAt = new Date('2026-10-20T08:00:00Z').toISOString(); }

let settings: T.Settings = { autoLockMinutes: 10, clipboardClearSecs: 90, theme: 'dark', language: 'en', biometricUnlock: false, quickAccessHotkey: 'CmdOrCtrl+Shift+Space', hibpCheck: false };
let onboarding: T.Onboarding = { extensionInstalled: false, emergencyKitSaved: false, repoConnected: true, imported: false, dismissed: false };
let unlocked = false;
const devices: T.DeviceInfo[] = [
  { id: 'd1', name: 'Demo MacBook (this device)', lastSeen: new Date().toISOString(), revoked: false, current: true },
  { id: 'd2', name: 'Example Linux Desktop', lastSeen: '2026-10-20T10:00:00Z', revoked: false, current: false },
];

const summary = (d: T.ItemDetail): T.ItemSummary => {
  const { sections: _s, notes: _n, attachments: _a, history: _h, passwordStrength: _p, hasTotp: _t, ...s } = d; return s;
};
const get = (id: string) => { const r = db.get(id); if (!r) throw new Error(`item not found: ${id}`); return r; };
const vaultInfo = () => vaults.map((v) => ({ ...v, itemCount: [...db.values()].filter((r) => r.detail.vaultId === v.id && !r.detail.deletedAt).length }));
const touch = (r: Rec) => {
  r.versions.push(structuredClone(r.detail));
  const now = new Date().toISOString(); r.detail.updatedAt = now;
  r.detail.history = [...r.detail.history, { version: r.detail.history.length + 1, updatedAt: now }];
};
const subtitleFor = (input: T.ItemInput): string => {
  const flds = input.sections.flatMap((s) => s.fields);
  const u = flds.find((x) => x.kind === 'text' || x.kind === 'email');
  return u?.value ?? '';
};
function applyInput(r: Rec, input: T.ItemInput) {
  const secrets: Record<string, string> = {};
  const sections = input.sections.map((s) => ({ ...s, id: s.id || uid('s'), fields: s.fields.map((x) => {
    const id = x.id || uid('f'); const hidden = x.kind === 'concealed' || x.kind === 'totp';
    // null concealed value on update = "unchanged"
    if (hidden) { const v = x.value ?? r.secrets[id]; if (v != null) secrets[id] = v; }
    return { ...x, id, value: hidden ? null : x.value };
  }) }));
  r.secrets = secrets;
  Object.assign(r.detail, { vaultId: input.vaultId, category: input.category, title: input.title, sections, notes: input.notes ?? null,
    tags: input.tags ?? [], favorite: input.favorite ?? r.detail.favorite, url: input.url ?? null, subtitle: subtitleFor(input) || r.detail.subtitle,
    hasTotp: sections.some((s) => s.fields.some((x) => x.kind === 'totp')) });
  const pw = sections.flatMap((s) => s.fields).find((x) => x.kind === 'concealed');
  r.detail.passwordStrength = pw && secrets[pw.id] ? strength(secrets[pw.id]) : null;
}
function genPw(length: number, symbols: boolean, digits: boolean, words?: boolean) {
  const rnd = (n: number) => crypto.getRandomValues(new Uint32Array(1))[0] % n;
  if (words) { const w = ['apple', 'river', 'stone', 'cloud', 'tiger', 'maple', 'ocean', 'pixel', 'lunar', 'cedar']; return Array.from({ length: Math.max(3, Math.round(length / 6)) }, () => w[rnd(w.length)]).join('-'); }
  const cs = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ' + (digits ? '23456789' : '') + (symbols ? '!@#$%^&*-_+=' : '');
  return Array.from({ length }, () => cs[rnd(cs.length)]).join('');
}

type A = any; // eslint-disable-line @typescript-eslint/no-explicit-any
export const mock: Record<string, (a: A) => Promise<unknown>> = {
  status: () => delay<T.UnlockState>({ unlocked, vaultPath: '~/.gitpass (mock)', account: unlocked ? 'Demo Family' : null }),
  unlock: (a: { password: string }) => { if (!a.password) return Promise.reject(new Error('password required')); unlocked = true; return delay<T.UnlockState>({ unlocked, vaultPath: '~/.gitpass (mock)', account: 'Demo Family' }, 300); },
  lock: () => { unlocked = false; return delay(undefined); },
  create_account: (a: { password: string }) => { if ((a.password ?? '').length < 8) return Promise.reject(new Error('master password must be at least 8 characters')); unlocked = true; return delay<T.CreateAccountResult>({ vaultPath: '~/.gitpass (mock)', secretKey: 'A3-MOCK00-000000-00000-00000-00000-00000' }, 300); },
  unlock_biometric: () => Promise.reject(new Error('biometric unlock is not supported yet on this platform')),
  list_vaults: () => delay(vaultInfo()),
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
    if (filter.query) { const q = filter.query.toLowerCase(); xs = xs.filter((x) => `${x.title} ${x.subtitle} ${x.url ?? ''} ${x.tags.join(' ')}`.toLowerCase().includes(q)); }
    const sort = filter.sort ?? 'updated';
    xs.sort((a, b) => sort === 'title' ? a.title.localeCompare(b.title) : sort === 'created' ? b.createdAt.localeCompare(a.createdAt) : b.updatedAt.localeCompare(a.updatedAt));
    return delay(xs.map(summary));
  },
  get_item: (a: { itemId: string }) => delay(get(a.itemId).detail),
  reveal_field: (a: { itemId: string; fieldId: string }) => { const v = get(a.itemId).secrets[a.fieldId]; return v == null ? Promise.reject(new Error('not a concealed field')) : delay(v); },
  copy_field: (a: { itemId: string; fieldId: string }) => {
    const r = get(a.itemId); const fld = r.detail.sections.flatMap((s) => s.fields).find((x) => x.id === a.fieldId);
    const v = r.secrets[a.fieldId] ?? fld?.value ?? '';
    // In Tauri the copy happens in Rust; the browser mock best-effort uses the clipboard API.
    navigator.clipboard?.writeText(v).catch(() => {});
    return delay(undefined);
  },
  totp_now: () => { const period = 30; const now = Math.floor(Date.now() / 1000); const code = String((Math.floor(now / period) * 7919) % 1000000).padStart(6, '0'); return delay({ code, remaining: period - (now % period), period }, 10); },
  create_item: ({ input }: { input: T.ItemInput }) => {
    const now = new Date().toISOString();
    const r: Rec = { secrets: {}, versions: [], detail: { id: uid('i'), vaultId: input.vaultId, category: input.category, title: input.title, subtitle: '', favorite: false, archived: false, deletedAt: null, tags: [], updatedAt: now, createdAt: now, url: null, sections: [], notes: null, attachments: [], history: [{ version: 1, updatedAt: now }], passwordStrength: null, hasTotp: false } };
    applyInput(r, input); db.set(r.detail.id, r); return delay(summary(r.detail));
  },
  update_item: ({ itemId, input }: { itemId: string; input: T.ItemInput }) => { const r = get(itemId); touch(r); applyInput(r, input); return delay(summary(r.detail)); },
  set_favorite: (a: { itemId: string; favorite: boolean }) => { get(a.itemId).detail.favorite = a.favorite; return delay(undefined); },
  archive_item: (a: { itemId: string; archived: boolean }) => { get(a.itemId).detail.archived = a.archived; return delay(undefined); },
  delete_item: (a: { itemId: string }) => { get(a.itemId).detail.deletedAt = new Date().toISOString(); return delay(undefined); },
  restore_item: (a: { itemId: string }) => { const d = get(a.itemId).detail; d.deletedAt = null; d.archived = false; return delay(undefined); },
  purge_item: (a: { itemId: string }) => { db.delete(a.itemId); return delay(undefined); },
  move_item: (a: { itemId: string; toVaultId: string }) => { const r = get(a.itemId); touch(r); r.detail.vaultId = a.toVaultId; return delay(summary(r.detail)); },
  duplicate_item: (a: { itemId: string }) => {
    const src = get(a.itemId); const r: Rec = structuredClone(src); const now = new Date().toISOString();
    r.versions = []; Object.assign(r.detail, { id: uid('i'), title: `${src.detail.title} (copy)`, createdAt: now, updatedAt: now, history: [{ version: 1, updatedAt: now }] });
    db.set(r.detail.id, r); return delay(summary(r.detail));
  },
  item_version: (a: { itemId: string; version: number }) => { const r = get(a.itemId); return delay(r.versions[a.version - 1] ?? r.detail); },
  add_attachment: (a: { itemId: string; path: string }) => { const att: T.Attachment = { id: uid('a'), name: a.path.split(/[\\/]/).pop() || 'file', size: 4096, mime: 'application/octet-stream' }; get(a.itemId).detail.attachments.push(att); return delay(att); },
  save_attachment: () => delay(undefined),
  list_tags: () => delay([...new Set([...db.values()].flatMap((r) => r.detail.tags))].sort()),
  generate_password: (a: { length: number; symbols: boolean; digits: boolean; words?: boolean }) => delay(genPw(a.length, a.symbols, a.digits, a.words), 5),
  password_strength: (a: { password: string }) => delay(strength(a.password), 5),
  watchtower: (a: { hibp: boolean }) => {
    const pw = [...db.values()].map((r) => { const fld = r.detail.sections.flatMap((s) => s.fields).find((x) => x.kind === 'concealed' && /password/.test(x.label)); return fld ? [r.detail.id, r.secrets[fld.id]] as const : null; }).filter(Boolean) as (readonly [string, string])[];
    const hibp = a.hibp && settings.hibpCheck; // like Rust: network check only when opted in
    const weak = pw.filter(([, p]) => strength4(p) < 2).map(([id]) => id);
    const groups = new Map<string, string[]>(); pw.forEach(([id, p]) => groups.set(p, [...(groups.get(p) ?? []), id]));
    const reused = [...groups.values()].filter((g) => g.length > 1);
    const logins = [...db.values()].filter((r) => r.detail.category === 'login');
    const missingTotp = logins.filter((r) => !r.detail.hasTotp).map((r) => r.detail.id);
    const old = logins.filter((r) => Date.parse(r.detail.updatedAt) < Date.parse('2026-09-01')).map((r) => r.detail.id);
    return delay<T.WatchtowerReport>({ score: Math.max(0, 100 - weak.length * 6 - reused.length * 8 - old.length * 2), weak, reused, old, breached: hibp ? weak.slice(0, 1) : [], missingTotp, checkedAt: new Date().toISOString(), hibpChecked: hibp }, 200);
  },
  import_file: (a: { dryRun: boolean }) => { if (!a.dryRun) onboarding = { ...onboarding, imported: true }; return delay<T.ImportResult>({ parsed: 12, added: a.dryRun ? 0 : 10, duplicates: 2, duplicateTitles: ['Example Mail', 'Example Mail'], warnings: ['row 4: skipped empty row'], categories: { login: 10, credit_card: 1, secure_note: 1 },
    preview: [{ title: 'Example Mail', category: 'login', username: 'alice@example.invalid', url: 'https://mail.example.invalid', hasPassword: true, hasTotp: true, tags: ['Work'], fields: 4, archived: false },
      { title: 'Fake Visa', category: 'credit_card', username: null, url: null, hasPassword: false, hasTotp: false, tags: [], fields: 5, archived: false }] }, 300); },
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
  sync_status: () => delay<T.SyncStatus>({ remote: 'git@example.com:demo/vault.git', lastSync: '2026-10-25T09:00:00Z', ahead: 0, behind: 1, dirty: false }),
  sync: () => delay({ message: 'Pulled 1 commit (mock)' }, 400),
  onboarding: () => delay(onboarding),
  set_onboarding: (a: { patch: Partial<T.Onboarding> }) => { onboarding = { ...onboarding, ...a.patch }; return delay(onboarding); },
  emergency_kit_data: (a: { password?: string }) => (a?.password !== undefined && a.password !== 'demo-password' ? Promise.reject(new Error('wrong master password'))
    : delay<T.EmergencyKitData>({ accountId: 'acc-demo-0000', vaultPath: '~/.gitpass (mock)', remote: 'git@example.com:demo/vault.git', deviceName: 'Demo Mac', createdAt: '2026-10-26T09:00:00Z', secretKeyAvailable: true, secretKey: a?.password ? 'A3-MOCK00-000000-00000-00000-00000-00000' : null })),
  emergency_kit: () => delay('gitpass Emergency Kit (DEMO)\nAccount: Demo Family\nSecret Key: (not shown — rotate to reveal a new one)'),
  favicon_url: () => delay(null),
};
