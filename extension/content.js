// gitpass content script: detect login forms, add an inline icon, fill only on explicit click.
/* global chrome, GitpassPolicy, GitpassI18n, GitpassIcons */
(function () {
  'use strict';
  if (window.top !== window) return;                         // never in iframes
  if (!GitpassPolicy.isFillableOrigin(location.origin)) return; // https or http://localhost only
  const MARK = 'data-gitpass';
  let menu = null; // { host, root }
  const LOGO = 'data:image/png;base64,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';
  const t = (k, f) => GitpassI18n.t(k, f);
  // All gitpass UI lives in closed shadow roots so page CSS can't restyle it and page scripts can't reach into it.
  const CSS = `:host{all:initial;position:absolute;z-index:2147483646}
  .badge{all:unset;box-sizing:border-box;width:22px;height:22px;border-radius:6px;cursor:pointer;display:flex;align-items:center;justify-content:center;background:#1C1C1E;box-shadow:0 1px 3px rgba(0,0,0,.35),0 0 0 1px rgba(242,140,40,.45);transition:transform .12s,box-shadow .12s}
  .badge:hover{transform:scale(1.08);box-shadow:0 2px 6px rgba(0,0,0,.4),0 0 0 2px #F28C28}
  .badge img{width:16px;height:16px;border-radius:4px;display:block}
  .menu{box-sizing:border-box;min-width:260px;max-width:340px;background:#1C1C1E;color:#ECEDF0;border:1px solid #3c3e44;border-radius:10px;box-shadow:0 12px 32px rgba(0,0,0,.45);font:13px/1.4 Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI","Noto Sans Myanmar","Noto Sans Thai",sans-serif;overflow:hidden}
  .hd{display:flex;align-items:center;gap:8px;padding:8px 10px;border-bottom:1px solid #3c3e44;font-size:11px;font-weight:600;letter-spacing:.5px;text-transform:uppercase;color:#969AA4}
  .hd img{width:16px;height:16px;border-radius:4px}
  .it{all:unset;box-sizing:border-box;display:flex;align-items:center;gap:10px;width:100%;padding:8px 10px;cursor:pointer}
  .it:hover,.it:focus-visible{background:#34363c}
  .m{min-width:0;flex:1}.ti{font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.u{font-size:12px;color:#969AA4;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
  .msg{padding:12px 10px;color:#969AA4}.err{color:#EF5350}`;

  function shadowHost() {
    const host = document.createElement('gitpass-ui');
    const root = host.attachShadow({ mode: 'closed' });
    const st = document.createElement('style'); st.textContent = CSS; root.append(st);
    document.documentElement.appendChild(host);
    return { host, root };
  }

  function closeMenu() { if (menu) { menu.host.remove(); menu = null; } }

  async function openMenu(anchor) {
    closeMenu();
    menu = shadowHost();
    menu.host.style.zIndex = '2147483647';
    const r = anchor.getBoundingClientRect();
    menu.host.style.left = Math.max(4, r.right + scrollX - 260) + 'px';
    menu.host.style.top = (r.bottom + scrollY + 6) + 'px';
    const box = document.createElement('div'); box.className = 'menu';
    const hd = document.createElement('div'); hd.className = 'hd';
    const lg = document.createElement('img'); lg.src = LOGO; lg.alt = '';
    hd.append(lg, document.createTextNode('gitpass'));
    const body = document.createElement('div'); body.className = 'msg'; body.textContent = '…';
    box.append(hd, body); menu.root.append(box);
    try {
      const { items } = await send({ type: 'cs-match' });
      if (!items.length) { body.textContent = t('csNoLogins', 'No gitpass logins for this site'); return; }
      body.remove();
      for (const it of items) {
        const b = document.createElement('button'); b.type = 'button'; b.className = 'it';
        const ic = document.createElement('span'); ic.innerHTML = GitpassIcons.categoryIcon('login', 28); // static SVG, no page data
        const m = document.createElement('span'); m.className = 'm';
        const ti = document.createElement('div'); ti.className = 'ti'; ti.textContent = it.title;
        const u = document.createElement('div'); u.className = 'u'; u.textContent = it.username || '—';
        m.append(ti, u); b.append(ic, m);
        b.addEventListener('click', async (e) => {
          if (!e.isTrusted) return; // explicit user click only
          try { await send({ type: 'cs-fill', item_id: it.id }); closeMenu(); }
          catch (err) { const d = document.createElement('div'); d.className = 'msg err'; d.textContent = err.message; box.append(d); }
        });
        box.append(b);
      }
    } catch (e) {
      body.className = 'msg err';
      body.textContent = /locked/i.test(e.message) ? t('csLocked', 'gitpass is locked. Unlock it from the toolbar icon.') : 'gitpass: ' + e.message;
    }
  }

  function decorate() {
    for (const { password } of GitpassPolicy.findLoginFields(document)) {
      if (password.hasAttribute(MARK)) continue;
      password.setAttribute(MARK, '1');
      const { host, root } = shadowHost();
      const btn = document.createElement('button');
      btn.type = 'button'; btn.className = 'badge'; btn.title = t('csFillWith', 'Fill with gitpass');
      const img = document.createElement('img'); img.src = LOGO; img.alt = 'gitpass'; btn.append(img);
      root.append(btn);
      const place = () => {
        const r = password.getBoundingClientRect();
        host.style.left = (r.right + scrollX - 28) + 'px';
        host.style.top = (r.top + scrollY + (r.height - 22) / 2) + 'px';
      };
      place();
      addEventListener('resize', place);
      btn.addEventListener('click', (e) => { if (e.isTrusted) { e.preventDefault(); e.stopPropagation(); openMenu(btn); } });
    }
  }

  // Fill request from background (after the host checked item URL vs origin).
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type !== 'gitpass-fill') return;
    if (msg.origin !== location.origin) return; // page navigated meanwhile: drop
    const fields = GitpassPolicy.findLoginFields(document);
    const f = fields[0];
    if (!f) return;
    setValue(f.username, msg.username);
    setValue(f.password, msg.password);
  });

  document.addEventListener('click', (e) => { if (menu && !e.composedPath().includes(menu.host)) closeMenu(); }, true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') closeMenu(); }, true);
  decorate();
  new MutationObserver(() => decorate()).observe(document.documentElement, { childList: true, subtree: true });
})();
