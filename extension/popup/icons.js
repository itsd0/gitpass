// Popup UI glyphs; category icons come from ../lib/icons.js (load it first).
(function (root) {
  'use strict';
  // Category glyphs live in lib/icons.js (shared with the in-page UI).
  const categoryIcon = (cat, size) => root.GitpassIcons.categoryIcon(cat, size);
  const monogramIcon = (title, size) => root.GitpassIcons.monogramIcon(title, size);
  const getMonogram = (title) => root.GitpassIcons.getMonogram(title);
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
    gear: s('<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>'),
    external: s('<path d="M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>'),
    link: s('<path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"/>'),
    type: s('<path d="M5 6h14M12 6v13M8.5 19h7"/>'),
    trash: s('<path d="M4 7h16M9 7V5h6v2M6.5 7l1 12.5a1.5 1.5 0 0 0 1.5 1.4h6a1.5 1.5 0 0 0 1.5-1.4L18 7M10 11v6M14 11v6"/>'),
    archive: s('<rect x="3" y="4" width="18" height="4" rx="1"/><path d="M5 8v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8M10 12h4"/>'),
    restore: s('<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8M3 3v5h5"/>'),
    alert: s('<circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>'),
    paperclip: s('<path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/>'),
    file: s('<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/>'),
    fileText: s('<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8M16 13H8M16 17H8"/>'),
    fileImage: s('<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><circle cx="10" cy="12" r="1.5"/><path d="m18 18-4.5-4.5-5.5 5.5"/>'),
    fileCode: s('<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="m10 13-2 2 2 2M14 17l2-2-2-2"/>'),
  };

  function fileIcon(name, mime) {
    const ext = (name && name.includes('.')) ? name.split('.').pop().toLowerCase() : '';
    const m = (mime || '').toLowerCase();
    if (m.startsWith('image/') || ['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'bmp', 'ico'].includes(ext)) {
      return UI.fileImage || UI.file;
    }
    if (['js', 'ts', 'jsx', 'tsx', 'rs', 'py', 'go', 'rb', 'php', 'c', 'cpp', 'h', 'java', 'json', 'yml', 'yaml', 'toml', 'xml', 'html', 'css', 'sh'].includes(ext)) {
      return UI.fileCode || UI.file;
    }
    if (m.startsWith('text/') || ['txt', 'md', 'csv', 'tsv', 'log', 'pdf'].includes(ext)) {
      return UI.fileText || UI.file;
    }
    return UI.file;
  }
  root.GitpassPopupIcons = { categoryIcon, monogramIcon, getMonogram, fileIcon, UI };
  if (typeof module !== 'undefined' && module.exports) module.exports = root.GitpassPopupIcons;
})(typeof globalThis !== 'undefined' ? globalThis : this);
