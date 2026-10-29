// Build a Firefox copy of the extension at dist/firefox (manifest.firefox.json -> manifest.json).
const fs = require('fs'), path = require('path');
const root = path.join(__dirname, '..'), out = path.join(root, 'dist', 'firefox');
fs.rmSync(out, { recursive: true, force: true });
for (const f of ['background.js', 'content.js', 'lib', 'popup', 'icons', '_locales', 'welcome', 'options', 'bg-onboard.js', 'bg-page.js', 'bg-passkey.js', 'bg-command.js', 'bg-context.js', 'bg-badge.js'])
  fs.cpSync(path.join(root, f), path.join(out, f), { recursive: true });
fs.copyFileSync(path.join(root, 'manifest.firefox.json'), path.join(out, 'manifest.json'));
// Fail the build if the manifest references a file we didn't copy (web-ext lint: MANIFEST_BACKGROUND_FILE_NOT_FOUND).
const m = JSON.parse(fs.readFileSync(path.join(out, 'manifest.json'), 'utf8'));
const refs = [...(m.background.scripts || []), ...m.content_scripts.flatMap((c) => c.js || []), m.action.default_popup, m.options_ui.page, ...Object.values(m.icons)];
const missing = refs.filter((f) => !fs.existsSync(path.join(out, f)));
if (missing.length) { console.error('missing in dist/firefox:', missing.join(', ')); process.exit(1); }
console.log('built', out);
