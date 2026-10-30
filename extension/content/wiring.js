// ---------- wiring ----------
// Part of the gitpass content script (see content.js): loaded from manifest.json in order,
// sharing one isolated-world global scope with the other content/*.js files. Code moved
// verbatim from the old single content.js; only the IIFE indent was removed.
'use strict';

// Side effects run once, and only when the guards in content.js passed.
if (window.__gitpassInpage && !window.__gitpassContentWired) {
  window.__gitpassContentWired = true;
  document.addEventListener('mousedown', (e) => {
    const p = e.composedPath();
    if (dd && !p.includes(dd.host) && p[0] !== dd.field) closeDropdown();
  }, true);
  addEventListener('resize', placeAll); addEventListener('scroll', placeAll, true);
  let scheduled = false;
  new MutationObserver((muts) => {
    const ours = (n) => n && n.nodeName === 'GITPASS-UI';
    if (muts.every((m) => ours(m.target) || (m.type === 'childList' && [...m.addedNodes, ...m.removedNodes].every(ours)))) return; // only our own UI changed
    pruneDetachedBadges();
    if (scheduled) return; scheduled = true; setTimeout(() => { scheduled = false; rescan(); }, 250); })
    .observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['style', 'class', 'hidden', 'type'] });
  rescan();
  send('page:pending').then(showBanner).catch(() => {});
}
