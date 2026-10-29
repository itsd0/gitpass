// Shared style-B category icons, all 8 v5 categories (popup + in-page) (same geometry as apps/desktop/src/components/CategoryIcon.tsx). Static SVG strings only.
(function (root) {
  'use strict';
  const A = '#F28C28', S = '#E5E7EB';
  const G = {
    login: ['<circle cx="8" cy="12" r="4"/><path d="M12 12h9M18 12v3M21 12v2"/>', `<circle cx="8" cy="12" r="1.3" fill="${A}"/>`],
    secure_note: ['<rect x="4.5" y="3.5" width="15" height="17" rx="2.5"/><path d="M8 8.5h8M8 12h8"/>', '<path d="M8 15.5h5"/>'],
    credit_card: ['<rect x="2.5" y="5.5" width="19" height="13" rx="2.5"/><path d="M2.5 10h19"/>', '<path d="M6 15h4"/>'],
    identity: ['<rect x="2.5" y="4.5" width="19" height="15" rx="2.5"/><path d="M5.5 16.5a3.5 3.5 0 0 1 6 0M14 10h4.5M14 13.5h3"/>', '<circle cx="8.5" cy="10.5" r="2"/>'],
    api_credential: ['<path d="M8 7l-5 5 5 5M16 7l5 5-5 5"/>', '<path d="M13.5 5.5l-3 13"/>'],
    document: ['<path d="M14 3H7.5A2.5 2.5 0 0 0 5 5.5v13A2.5 2.5 0 0 0 7.5 21h9a2.5 2.5 0 0 0 2.5-2.5V8z"/><path d="M8.5 13h7M8.5 16.5h5"/>', '<path d="M14 3v5h5" stroke-linejoin="round"/>'],
    ssh_key: ['<rect x="2.5" y="4.5" width="19" height="15" rx="2.5"/><path d="M6.5 9.5l3 2.5-3 2.5"/>', '<path d="M12 15h5"/>'],
    passkey: ['<circle cx="9" cy="8" r="3.5"/><path d="M3 20a6 6 0 0 1 10.5-4"/>', '<circle cx="17.5" cy="13.5" r="2.5"/><path d="M17.5 16v5M17.5 18.5h2"/>'],
    wifi: [`<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0"/><circle cx="12" cy="19.2" r="1.1" fill="${S}" stroke="none"/>`, '<path d="M8.5 16a5 5 0 0 1 7 0"/>'],
    bank_account: ['<path d="m3 9 9-7 9 7v2H3V9z"/><path d="M4 22h16"/><path d="M19 11v9M5 11v9M10 11v9M14 11v9"/>', `<circle cx="12" cy="5.5" r="1.3" fill="${A}"/>`],
    server: ['<rect x="2.5" y="3.5" width="19" height="7" rx="2"/><rect x="2.5" y="13.5" width="19" height="7" rx="2"/><path d="M6 7h.01M6 17h.01"/>', `<circle cx="18" cy="7" r="1.2" fill="${A}"/><circle cx="18" cy="17" r="1.2" fill="${A}"/>`],
  };
  function categoryIcon(cat, size) {
    const g = G[cat] || G.secure_note, sw = size <= 20 ? 1.9 : size <= 28 ? 1.7 : 1.5;
    return `<svg viewBox="0 0 64 64" width="${size}" height="${size}" aria-hidden="true" style="flex-shrink:0">` +
      `<circle cx="32" cy="32" r="30" fill="${A}" fill-opacity="0.14"/><circle cx="32" cy="32" r="29.5" fill="none" stroke="${A}" stroke-opacity="0.3"/>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${S}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round">${g[0]}</g>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${A}" stroke-width="${sw}" stroke-linecap="round">${g[1]}</g></svg>`;
  }
  root.GitpassIcons = { categoryIcon, CATEGORIES: Object.keys(G) };
  if (typeof module !== 'undefined' && module.exports) module.exports = root.GitpassIcons;
})(typeof globalThis !== 'undefined' ? globalThis : this);
