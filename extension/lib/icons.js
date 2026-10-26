// Style-B category icon (same geometry as apps/desktop/src/components/CategoryIcon.tsx). Static SVG strings only.
(function (root) {
  'use strict';
  const A = '#F28C28', S = '#E5E7EB';
  const G = {
    login: ['<circle cx="8" cy="12" r="4"/><path d="M12 12h9M18 12v3M21 12v2"/>', `<circle cx="8" cy="12" r="1.3" fill="${A}"/>`],
    secure_note: ['<rect x="4.5" y="3.5" width="15" height="17" rx="2.5"/><path d="M8 8.5h8M8 12h8"/>', '<path d="M8 15.5h5"/>'],
  };
  function categoryIcon(cat, size) {
    const g = G[cat] || G.login, sw = size <= 20 ? 1.9 : size <= 28 ? 1.7 : 1.5;
    return `<svg viewBox="0 0 64 64" width="${size}" height="${size}" aria-hidden="true" style="flex-shrink:0">` +
      `<circle cx="32" cy="32" r="30" fill="${A}" fill-opacity="0.14"/><circle cx="32" cy="32" r="29.5" fill="none" stroke="${A}" stroke-opacity="0.3"/>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${S}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round">${g[0]}</g>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${A}" stroke-width="${sw}" stroke-linecap="round">${g[1]}</g></svg>`;
  }
  root.GitpassIcons = { categoryIcon };
})(typeof globalThis !== 'undefined' ? globalThis : this);
