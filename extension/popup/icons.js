// Popup icons: all 8 style-B category glyphs ported from apps/desktop/src/components/CategoryIcon.tsx, plus UI glyphs.
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
    wifi: [`<path d="M2.5 9a14 14 0 0 1 19 0M5.5 12.5a9.5 9.5 0 0 1 13 0"/><circle cx="12" cy="19.2" r="1.1" fill="${S}" stroke="none"/>`, '<path d="M8.5 16a5 5 0 0 1 7 0"/>'],
  };
  function categoryIcon(cat, size) {
    const g = G[cat] || G.secure_note, sw = size <= 20 ? 1.9 : size <= 28 ? 1.7 : 1.5;
    return `<svg viewBox="0 0 64 64" width="${size}" height="${size}" aria-hidden="true" style="flex-shrink:0">` +
      `<circle cx="32" cy="32" r="30" fill="${A}" fill-opacity="0.14"/><circle cx="32" cy="32" r="29.5" fill="none" stroke="${A}" stroke-opacity="0.3"/>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${S}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round">${g[0]}</g>` +
      `<g transform="translate(14 14) scale(1.5)" fill="none" stroke="${A}" stroke-width="${sw}" stroke-linecap="round">${g[1]}</g></svg>`;
  }
  const s = (d) => `<svg class="ui" viewBox="0 0 24 24" aria-hidden="true">${d}</svg>`;
  const UI = {
    copy: s('<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>'),
    eye: s('<path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>'),
    eyeOff: s('<path d="M3 3l18 18M10.6 5.1A10 10 0 0 1 12 5c6.5 0 10 7 10 7a17 17 0 0 1-3.2 4M6.6 6.6A17 17 0 0 0 2 12s3.5 7 10 7a9.6 9.6 0 0 0 5.4-1.6"/><path d="M9.9 9.9a3 3 0 0 0 4.2 4.2"/>'),
    key: s('<circle cx="8" cy="15" r="4"/><path d="M11 12l8-8M16 7l3 3M14 9l2 2"/>'),
    plus: s('<path d="M12 5v14M5 12h14"/>'),
    search: s('<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>'),
    lock: s('<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>'),
    more: s('<circle cx="5" cy="12" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="19" cy="12" r="1.4"/>'),
    star: s('<path d="M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.8L12 16.8 6.8 19.6l1-5.8L3.5 9.7l5.9-.8z"/>'),
    regen: s('<path d="M20 12a8 8 0 1 1-2.3-5.6M20 4v4h-4"/>'),
    chevron: s('<path d="M6 9l6 6 6-6"/>'),
    fingerprint: s('<path d="M12 11v3a8 8 0 0 1-1.5 4.7M8.5 7.6A5 5 0 0 1 17 11v1.5M7 11a5 5 0 0 1 .4-2M7 14.5a12 12 0 0 1-.9 3.5M17 15.5a17 17 0 0 1-.8 3.5M14.5 11v2.5a11 11 0 0 1-1.6 5.8M4.5 9a8 8 0 0 1 15 0M9.5 11v2a10 10 0 0 1-1.2 4.8"/>'),
    close: s('<path d="M6 6l12 12M18 6L6 18"/>'),
    external: s('<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>'),
    link: s('<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>'),
  };
  root.GitpassPopupIcons = { categoryIcon, UI };
})(typeof globalThis !== 'undefined' ? globalThis : this);
