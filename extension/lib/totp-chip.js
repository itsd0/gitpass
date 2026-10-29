// The "Fill one-time code" chip shown under a code field on the page after gitpass filled a login (#181).
// DOM only, no chrome.* and no secrets: content.js owns the shadow root, positioning and the fill itself.
// Enter / click on the chip fills, Esc or the dismiss button closes it, Tab reaches both buttons, and a
// countdown ring shows how long the offer stays up (it removes itself at the end).
(function (root) {
  'use strict';
  const SVG_NS = 'http://www.w3.org/2000/svg';
  const R = 8, CIRC = 2 * Math.PI * R;

  /** 'fill' | 'dismiss' | null for a key pressed while the chip has focus. */
  function keyAction(key) {
    if (key === 'Enter') return 'fill';
    if (key === 'Escape') return 'dismiss';
    return null;
  }

  function ring(doc, ttlMs) {
    const svg = doc.createElementNS(SVG_NS, 'svg');
    svg.setAttribute('class', 'tc-ring'); svg.setAttribute('viewBox', '0 0 20 20'); svg.setAttribute('aria-hidden', 'true');
    const track = doc.createElementNS(SVG_NS, 'circle');
    const arc = doc.createElementNS(SVG_NS, 'circle');
    for (const c of [track, arc]) { c.setAttribute('cx', '10'); c.setAttribute('cy', '10'); c.setAttribute('r', String(R)); c.setAttribute('fill', 'none'); c.setAttribute('stroke-width', '2'); }
    track.setAttribute('class', 'tc-track');
    arc.setAttribute('class', 'tc-arc');
    arc.setAttribute('stroke-dasharray', CIRC.toFixed(2));
    arc.style.animationDuration = Math.max(0, Math.round(ttlMs)) + 'ms';
    svg.append(track, arc);
    return svg;
  }

  /**
   * o: { title, label, dismissLabel, logo, ttlMs, onFill(), onDismiss(), trusted(e) }
   * `trusted` defaults to e.isTrusted: a page script can never fill by dispatching synthetic events.
   * Returns { el, close() }; close() stops the timer (onDismiss is not called by close()).
   */
  function build(doc, o) {
    const trusted = o.trusted || ((e) => e.isTrusted);
    const chip = doc.createElement('div'); chip.className = 'tc'; chip.setAttribute('role', 'group');
    chip.setAttribute('aria-label', o.label);
    const fill = doc.createElement('button'); fill.type = 'button'; fill.className = 'tc-fill';
    const img = doc.createElement('img'); img.src = o.logo || ''; img.alt = '';
    const txt = doc.createElement('span'); txt.className = 'm';
    const ti = doc.createElement('span'); ti.className = 'ti'; ti.textContent = o.label;
    const u = doc.createElement('span'); u.className = 'u'; u.textContent = o.title || '';
    txt.append(ti, u);
    fill.append(img, txt, ring(doc, o.ttlMs));
    const x = doc.createElement('button'); x.type = 'button'; x.className = 'tc-x'; x.textContent = '×';
    x.setAttribute('aria-label', o.dismissLabel); x.title = o.dismissLabel;
    chip.append(fill, x);

    let done = false;
    const timer = setTimeout(() => { if (!done) { done = true; o.onDismiss(); } }, Math.max(0, o.ttlMs));
    const end = () => { done = true; clearTimeout(timer); };
    const doFill = () => { if (done) return; end(); o.onFill(); };
    const doDismiss = () => { if (done) return; end(); o.onDismiss(); };
    fill.addEventListener('mousedown', (e) => e.preventDefault()); // keep focus in the field on mouse use
    fill.addEventListener('click', (e) => { if (!trusted(e)) return; e.preventDefault(); e.stopPropagation(); doFill(); });
    x.addEventListener('click', (e) => { if (!trusted(e)) return; e.preventDefault(); e.stopPropagation(); doDismiss(); });
    chip.addEventListener('keydown', (e) => {
      if (!trusted(e)) return;
      const a = keyAction(e.key);
      if (a === 'dismiss') { e.preventDefault(); e.stopPropagation(); doDismiss(); }
      else if (a === 'fill' && e.target === fill) { e.preventDefault(); e.stopPropagation(); doFill(); }
    });
    return { el: chip, fill, dismissBtn: x, close: end };
  }

  const api = { build, keyAction };
  root.GitpassTotpChip = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
