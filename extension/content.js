// gitpass content script: detect login forms, add an inline icon, fill only on explicit click.
/* global chrome, GitpassPolicy */
(function () {
  'use strict';
  if (window.top !== window) return;                         // never in iframes
  if (!GitpassPolicy.isFillableOrigin(location.origin)) return; // https or http://localhost only
  const MARK = 'data-gitpass';
  let menu = null;

  function send(msg) {
    return new Promise((res, rej) => chrome.runtime.sendMessage(msg, (r) => {
      if (chrome.runtime.lastError) return rej(new Error(chrome.runtime.lastError.message));
      r && r.ok ? res(r.result) : rej(new Error((r && r.error) || 'error'));
    }));
  }

  function setValue(el, v) {
    if (!el || v == null) return;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    setter.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  }

  function closeMenu() { if (menu) { menu.remove(); menu = null; } }

  async function openMenu(btn) {
    closeMenu();
    menu = document.createElement('div');
    menu.style.cssText = 'position:absolute;z-index:2147483647;background:#fff;color:#111;border:1px solid #888;border-radius:6px;font:13px sans-serif;min-width:200px;box-shadow:0 2px 8px #0004';
    const r = btn.getBoundingClientRect();
    menu.style.left = (r.left + scrollX) + 'px';
    menu.style.top = (r.bottom + scrollY + 4) + 'px';
    menu.textContent = 'gitpass…';
    document.body.appendChild(menu);
    try {
      const { items } = await send({ type: 'cs-match' });
      menu.textContent = '';
      if (!items.length) menu.textContent = 'No gitpass logins for this site';
      for (const it of items) {
        const b = document.createElement('button');
        b.type = 'button';
        b.style.cssText = 'display:block;width:100%;text-align:left;padding:6px 10px;border:0;background:none;cursor:pointer';
        b.textContent = it.title + (it.username ? ' — ' + it.username : '');
        b.addEventListener('click', async (e) => {
          if (!e.isTrusted) return; // explicit user click only
          try { await send({ type: 'cs-fill', item_id: it.id }); } catch (err) { alert('gitpass: ' + err.message); }
          closeMenu();
        });
        menu.appendChild(b);
      }
    } catch (e) {
      menu.textContent = 'gitpass: ' + e.message + ' (unlock via the toolbar popup)';
    }
  }

  function decorate() {
    for (const { password } of GitpassPolicy.findLoginFields(document)) {
      if (password.hasAttribute(MARK)) continue;
      password.setAttribute(MARK, '1');
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.title = 'Fill with gitpass';
      btn.textContent = '🔑';
      btn.style.cssText = 'position:absolute;z-index:2147483646;border:0;background:transparent;cursor:pointer;font-size:14px;padding:0';
      const place = () => {
        const r = password.getBoundingClientRect();
        btn.style.left = (r.right + scrollX - 22) + 'px';
        btn.style.top = (r.top + scrollY + (r.height - 18) / 2) + 'px';
      };
      place();
      addEventListener('resize', place);
      btn.addEventListener('click', (e) => { if (e.isTrusted) { e.preventDefault(); openMenu(btn); } });
      document.body.appendChild(btn);
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

  document.addEventListener('click', (e) => { if (menu && !menu.contains(e.target)) closeMenu(); }, true);
  decorate();
  new MutationObserver(() => decorate()).observe(document.documentElement, { childList: true, subtree: true });
})();
