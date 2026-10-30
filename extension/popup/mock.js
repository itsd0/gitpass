// Mock backend for popup.html?mock=1 (fake data only; used for previews and screenshots).
// Extra params: &view=locked|appdown|nohost  &item=<id>  &filter=<filter id>
(function (root) {
  'use strict';
  const P = new URLSearchParams((root.location && root.location.search) || '');
  if (P.get('mock') !== '1') return;
  const now = Date.now();
  const V = [{ id: 'v1', name: 'Personal', item_count: 0 }, { id: 'v2', name: 'Work', item_count: 0 }];
  const raw = [
    ['v1', 'login', 'GitHub', 'naing@example.com', 'https://github.com', true, true],
    ['v2', 'login', 'GitHub (work)', 'naing-work', 'https://github.com', true, false],
    ['v1', 'login', 'Google', 'naing.demo@gmail.com', 'https://accounts.google.com', false, true],
    ['v1', 'login', 'Netflix', 'family@example.com', 'https://netflix.com', false, false],
    ['v2', 'login', 'Vercel', 'naing@acme.dev', 'https://vercel.com', true, false],
    ['v1', 'login', 'Amazon', 'naing@example.com', 'https://amazon.com', false, false],
    ['v1', 'login', 'Grab', '+66 81 234 5678', 'https://grab.com', false, false],
    ['v2', 'login', 'AWS Console', 'admin', 'https://aws.amazon.com', false, true],
    ['v1', 'credit_card', 'Visa Platinum', 'Visa •••• 4242', null, true, false],
    ['v1', 'credit_card', 'Mastercard', 'Mastercard •••• 5100', null, false, false],
    ['v1', 'identity', 'Naing Oo', 'Naing Oo', null, false, false],
    ['v1', 'secure_note', 'Recovery codes', 'Updated last week', null, false, false],
    ['v2', 'api_credential', 'OpenAI API', 'sk-…', null, false, false],
    ['v2', 'document', 'Passport scan', 'passport.pdf · 1.2 MB', null, false, false],
    ['v2', 'ssh_key', 'Deploy key', 'ed25519', null, false, false],
    ['v1', 'wifi', 'Home Wi-Fi', 'NaingHome-5G', null, false, false],
  ];
  for (let i = 1; i <= 30; i++) raw.push(['v1', 'login', 'Demo site ' + i, 'user' + i + '@example.com', 'https://demo' + i + '.example', false, false]);
  const items = raw.map(([v, category, title, subtitle, url, favorite, has_totp], i) => ({
    id: v + ':i' + i, vault_id: v, vault_name: V.find((x) => x.id === v).name, category, title, subtitle,
    url: url || undefined, favicon_host: url ? new URL(url).hostname : undefined, has_totp,
    has_attachments: category === 'document' || i === 0, favorite, updated_at: now - i * 3600e3,
    tags: i === 0 || i === 1 ? ['dev'] : i === 2 ? ['personal'] : i === 4 ? ['dev', 'cloud'] : i === 3 ? ['family'] : [],
  }));
  for (const v of V) v.item_count = items.filter((i) => i.vault_id === v.id).length;
  const trashed = new Set(['v1:i6']);
  const archived = new Set(['v2:i14']);
  const liveItems = () => items.filter((i) => !trashed.has(i.id) && !archived.has(i.id));
  const liveItem = (id) => items.find((i) => i.id === id && !trashed.has(i.id));
  const F = (name, label, kind, value, concealed) => ({ name, label, kind, concealed: !!concealed, value: concealed ? undefined : value });
  const secrets = {};
  function detail(it) {
    let fields;
    switch (it.category) {
      case 'login': fields = [F('username', 'username', 'text', it.subtitle), F('password', 'password', 'password', null, true), F('url', 'website', 'url', it.url)];
        secrets.password = 'c0rrect-Horse-battery!'; if (it.has_totp) fields.push(F('totp', 'one-time password', 'totp', null, true)); break;
      case 'credit_card': fields = [F('cardholder', 'cardholder name', 'text', 'NAING OO'), Object.assign(F('number', 'number', 'card_number', null, true), { masked: '•••• •••• •••• ' + String(it.subtitle).slice(-4) }), F('expiry', 'expiry date', 'date', '08/2029'), F('cvv', 'verification number', 'password', null, true), F('type', 'type', 'text', 'Visa')];
        secrets.number = '4242424242424242'; secrets.cvv = '123'; break;
      case 'identity': fields = [F('first', 'first name', 'text', 'Naing'), F('last', 'last name', 'text', 'Oo'), F('email', 'email', 'email', 'naing@example.com'), F('phone', 'phone', 'phone', '+66 81 234 5678'), F('birth', 'birth date', 'date', '1990-04-12')]; break;
      case 'secure_note': fields = []; break;
      case 'api_credential': fields = [F('username', 'username', 'text', 'naing'), F('credential', 'credential', 'password', null, true), F('host', 'hostname', 'url', 'https://api.openai.com')]; secrets.credential = 'sk-demo-1234567890'; break;
      case 'document': fields = [F('file', 'file', 'text', 'passport.pdf (1.2 MB)')]; break;
      case 'ssh_key': fields = [F('public', 'public key', 'text', 'ssh-ed25519 AAAAC3Nza…demo naing@mac'), F('private', 'private key', 'password', null, true), F('fp', 'fingerprint', 'text', 'SHA256:x2Tq…9fA')]; secrets.private = '-----BEGIN OPENSSH PRIVATE KEY-----demo'; break;
      case 'wifi': fields = [F('ssid', 'network name', 'text', 'NaingHome-5G'), F('password', 'wireless password', 'password', null, true), F('security', 'security', 'text', 'WPA3')]; secrets.password = 'wifi-demo-pass'; break;
    }
    const attachments = (it && it.has_attachments) ? [
      { id: 'att-1', name: 'passport-scan.pdf', size: 1258291, mime: 'application/pdf' },
    ] : [];
    const dAt = trashed.has(it.id) ? new Date(now - 864e5 * 2).toISOString() : undefined;
    return { ...it, fields, attachments, archived: archived.has(it.id), deleted_at: dAt, notes: it.notes != null ? it.notes : (it.category === 'secure_note' ? 'Demo recovery codes:\n1111-2222\n3333-4444' : ''), tags: ['demo'], created_at: now - 864e5 * 30 };
  }
  let unlocked = !['locked', 'appdown', 'nohost'].includes(P.get('view'));
  const appRunning = P.get('view') !== 'appdown';
  const ok = (r) => new Promise((res) => setTimeout(() => res(r), 20));
  const err = (code) => Promise.reject(Object.assign(new Error(code), { code }));
  root.__gitpassMockParams = P;
  root.__gitpassMock = function (msg) {
    const t = msg.type.replace(/^popup:/, '');
    if (P.get('view') === 'nohost') return err('Specified native messaging host not found.');
    if (t === 'status') return ok({ unlocked, app_running: appRunning, account: { name: 'Naing Oo', email: 'naing@example.com', vault_count: 2 }, idle_timeout_secs: 600 });
    if (t === 'request_unlock' || t === 'unlock') { unlocked = true; return ok({ unlocked: true }); }
    if (t === 'open_in_app') return ok({});
    if (!unlocked) return err('locked');
    switch (t) {
      case 'lock': unlocked = false; return ok({ unlocked: false });
      case 'site_icon': {
        const h = String(msg.host || '').toLowerCase();
        if (h === 'github.com') {
          return ok({ data_url: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PGNpcmNsZSBjeD0iMTIiIGN5PSIxMiiciHI9IjEwIiBmaWxsPSIjZjI4YzI4Ii8+PC9zdmc+' });
        }
        return err('not_found');
      }
      case 'list_vaults': return ok({ vaults: V });
      case 'list_items': return ok({ items: liveItems() });
      case 'list_archived': return ok({ items: items.filter((i) => archived.has(i.id) && !trashed.has(i.id)).map((i) => ({ ...i, archived: true })) });
      case 'list_deleted': return ok({ items: items.filter((i) => trashed.has(i.id)).map((i) => ({ ...i, deleted_at: new Date(now - 864e5 * 2).toISOString() })) });
      case 'match_active': return ok({ origin: 'https://github.com', items: liveItems().filter((i) => i.favicon_host === 'github.com') });
      case 'get_item': {
        const it = items.find((i) => i.id === msg.item_id);
        if (!it || (trashed.has(it.id) && !msg.include_deleted)) return err('not_found');
        return ok({ item: detail(it) });
      }
      case 'item_findings': {
        const it = liveItem(msg.item_id);
        if (!it) return err('not_found');
        if (it.id === 'v1:i0') {
          return ok({
            kinds: ['weak', 'reused'],
            reused_count: 3,
            change_password_url: 'https://github.com/.well-known/change-password',
          });
        }
        return ok({ kinds: [] });
      }
      case 'reveal': {
        const it = liveItem(msg.item_id);
        if (!it) return err('not_found');
        detail(it);
        if (msg.field && msg.field.startsWith('password_history:')) {
          const idx = parseInt(msg.field.split(':')[1], 10);
          const val = (it.password_history && it.password_history[idx]?.value) || secrets[msg.field] || 'fake-old-pw';
          return ok({ value: val });
        }
        return ok({ value: secrets[msg.field] || '' });
      }
      case 'totp': { const s = Math.floor(Date.now() / 1000); return ok({ code: String(100000 + (Math.floor(s / 30) * 7919) % 900000), remaining: 30 - (s % 30), period: 30 }); }
      case 'generate': return ok({ password: 'Xk7#pQ2m-Vr9!tLz4wNb' });
      case 'generator_history': {
        const n = Math.floor(Date.now() / 1000);
        return ok({ enabled: true, entries: [
          { id: 'h1', value: 'Fake-Pw-aB3$kLm9QzXv', created_at: n - 120, source: 'extension', host: 'github.com' },
          { id: 'h2', value: 'Fake-Pw-7rTn!eWp2YcD', created_at: n - 3 * 3600, source: 'extension', host: 'example.invalid' },
          { id: 'h3', value: 'Fake-Pw-Zq8&uHs4LbNf', created_at: n - 2 * 86400, source: 'desktop' },
        ] });
      }
      case 'save_login': return ok({ item_id: 'v1:new' });
      case 'update_login': {
        const it = items.find((i) => i.id === msg.item_id);
        if (!it) return err('not_found');
        if (msg.title != null) it.title = msg.title;
        if (msg.username != null) it.subtitle = msg.username;
        if (msg.password != null) secrets.password = msg.password;
        if (msg.urls) it.url = msg.urls[0] || null;
        if (msg.notes != null) it.notes = msg.notes;
        it.updated_at = Date.now();
        return ok({ ok: true });
      }
      case 'create_item': { const id = 'v1:new' + items.length; items.unshift({ id, vault_id: 'v1', title: msg.item.title, subtitle: '', category: msg.item.category, favorite: false }); return ok({ item_id: id, title: msg.item.title }); }
      case 'set_favorite': { const it = items.find((i) => i.id === msg.item_id); if (!it) return err('not_found'); it.favorite = !!msg.favorite; return ok({ ok: true }); }
      case 'trash_item': {
        const it = liveItem(msg.item_id);
        if (!it) return err('not_found');
        trashed.add(it.id);
        return ok({ ok: true });
      }
      case 'restore_item': {
        if (!items.some((x) => x.id === msg.item_id)) return err('not_found');
        trashed.delete(msg.item_id);
        return ok({ ok: true });
      }
      case 'archive_item': {
        if (!items.some((x) => x.id === msg.item_id)) return err('not_found');
        archived.add(msg.item_id);
        return ok({ ok: true, archived: true });
      }
      case 'unarchive_item': {
        if (!items.some((x) => x.id === msg.item_id)) return err('not_found');
        archived.delete(msg.item_id);
        return ok({ ok: true, archived: false });
      }
      case 'open_fill': case 'fill_active': return ok(true);
    }
    return err('bad_request');
  };
})(typeof globalThis !== 'undefined' ? globalThis : this);
