// Build a Firefox copy of the extension at dist/firefox (manifest.firefox.json -> manifest.json).
const fs = require('fs'), path = require('path');
const root = path.join(__dirname, '..'), out = path.join(root, 'dist', 'firefox');
fs.rmSync(out, { recursive: true, force: true });
for (const f of ['background.js', 'content.js', 'lib', 'popup', 'icons', '_locales', 'welcome', 'options', 'bg-onboard.js'])
  fs.cpSync(path.join(root, f), path.join(out, f), { recursive: true });
fs.copyFileSync(path.join(root, 'manifest.firefox.json'), path.join(out, 'manifest.json'));
console.log('built', out);
