// Syntax-check every extension script and validate manifest.json references (no deps).
const fs = require('fs'), path = require('path'), vm = require('vm');
const root = path.join(__dirname, '..');
const m = JSON.parse(fs.readFileSync(path.join(root, 'manifest.json'), 'utf8'));
const files = new Set(['background.js', 'content.js', 'lib/policy.js', 'popup/popup.js']);
for (const cs of m.content_scripts) cs.js.forEach((f) => files.add(f));
let bad = 0;
for (const f of files) {
  const p = path.join(root, f);
  if (!fs.existsSync(p)) { console.error('missing', f); bad++; continue; }
  try { new vm.Script(fs.readFileSync(p, 'utf8'), { filename: f }); console.log('ok', f); } catch (e) { console.error('syntax', f, e.message); bad++; }
}
for (const f of [m.background.service_worker, m.action.default_popup, ...Object.values(m.icons)])
  if (!fs.existsSync(path.join(root, f))) { console.error('missing', f); bad++; }
if (m.manifest_version !== 3) { console.error('not MV3'); bad++; }
if (!m.permissions.includes('nativeMessaging')) { console.error('no nativeMessaging'); bad++; }
if (m.background.scripts) { console.error('Chrome manifest must not use background.scripts'); bad++; }
const ff = JSON.parse(fs.readFileSync(path.join(root, 'manifest.firefox.json'), 'utf8'));
if (ff.version !== m.version) { console.error('firefox/chrome version mismatch'); bad++; }
for (const f of ff.background.scripts || []) if (!fs.existsSync(path.join(root, f))) { console.error('missing', f); bad++; }
if (ff.background.service_worker) { console.error('firefox manifest should use background.scripts'); bad++; }
process.exit(bad ? 1 : 0);
