/* global chrome, GitpassOnboard, GitpassI18n */
'use strict';
const $ = (id) => document.getElementById(id);
const t = (k, f) => (globalThis.GitpassI18n ? GitpassI18n.t(k, f) : f);
const done = { connect: false, pin: false, try: false };

function render() {
  for (const k of Object.keys(done)) $('s-' + k).classList.toggle('done', done[k]);
  const n = Object.values(done).filter((v) => !v).length;
  $('left').textContent = n === 0 ? t('allSet', 'All set') : n === 1 ? t('oneStepLeft', '1 step left') : t('stepsLeft', '{n} steps left').replace('{n}', n);
  chrome.storage.local.set({ onboarding: done });
}

async function check() {
  const st = $('connect-status'), help = $('connect-help');
  st.className = 'status'; st.textContent = t('checking', 'Checking…'); help.hidden = true;
  let r;
  try { r = await GitpassOnboard.send({ type: 'onboard-probe' }); } catch (e) { r = { host: false, error: e.message }; }
  if (!r.host) {
    st.className = 'status bad'; st.textContent = t('hostMissing', 'gitpass helper not found.');
    help.hidden = false;
    help.innerHTML = '';
    help.append(t('hostMissingHelp', 'Install it once from the gitpass app (Settings → Browser extension → Install helper), or in Terminal run '));
    const c = document.createElement('code'); c.textContent = 'gitpass-host install'; help.append(c, '. ' + t('thenRetry', 'Then press Retry.'));
    done.connect = false;
  } else if (!r.app) {
    st.className = 'status bad'; st.textContent = t('appNotRunning', 'Helper found, but the gitpass app is not running.');
    help.hidden = false; help.textContent = t('appNotRunningHelp', 'Open the gitpass app and sign in, then press Retry.');
    done.connect = false;
  } else {
    st.className = 'status good';
    st.textContent = r.unlocked ? t('connectedUnlocked', 'Connected. gitpass is unlocked.') : t('connectedLocked', 'Connected. Unlock the gitpass app to use the extension.');
    done.connect = true;
  }
  render();
}

async function init() {
  if (globalThis.GitpassI18n) GitpassI18n.apply(document);
  const saved = await new Promise((res) => chrome.storage.local.get('onboarding', (r) => res((r && r.onboarding) || {})));
  done.pin = !!saved.pin; done.try = !!saved.try;
  try { const s = await GitpassOnboard.send({ type: 'onboard-shortcut' }); if (s.shortcut) $('shortcut').textContent = s.shortcut; } catch (_) { /* ignore */ }
  try { const p = await GitpassOnboard.send({ type: 'onboard-pinned' }); if (p.pinned) done.pin = true; } catch (_) { /* ignore */ }
  $('retry').onclick = check;
  $('open-app').onclick = () => GitpassOnboard.send({ type: 'onboard-open-app' }).then(() => setTimeout(check, 1500)).catch(() => {});
  $('pin-done').onclick = () => { done.pin = true; render(); };
  const mark = () => { if ($('demo-user').value && $('demo-pass').value) { done.try = true; $('try-status').className = 'status good'; $('try-status').textContent = t('tryDone', 'Filled. Autofill works.'); render(); } };
  ['input', 'change'].forEach((ev) => { $('demo-user').addEventListener(ev, mark); $('demo-pass').addEventListener(ev, mark); });
  setInterval(mark, 1000); // programmatic fills may not fire input events
  $('demo').onsubmit = (e) => { e.preventDefault(); mark(); };
  render();
  check();
}
init();
