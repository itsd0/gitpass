// Tests for Bug 2: multi-step login username capture, preservation across steps,
// end-to-end save payload with both username and password, and "update existing" option.
const test = require('node:test');
const assert = require('node:assert');
const { JSDOM } = require('jsdom');

globalThis.GitpassPolicy = require('../lib/policy.js');
const forms = require('../lib/forms.js');
const B = require('../bg-page.js');

test('Bug 2: hidden and disabled username fields are detected and captured', () => {
  const dom = new JSDOM(`<!doctype html><html><body>
    <form id="step2">
      <input type="hidden" name="username" value="ReiReieth">
      <input type="password" name="password" value="p@ssw0rd123">
      <button type="submit">Log in</button>
    </form>
  </body></html>`, { url: 'https://x.com/login' });

  const form = forms.detectForms(dom.window.document, { ignoreLayout: true })[0];
  assert.ok(form, 'form detected');
  assert.strictEqual(form.kind, 'login');
  assert.ok(form.fields.username, 'hidden username field found');
  assert.strictEqual(form.fields.username.value, 'ReiReieth');

  const creds = forms.captureCredentials(form);
  assert.deepStrictEqual(creds, {
    kind: 'login',
    username: 'ReiReieth',
    password: 'p@ssw0rd123'
  });
  dom.window.close();
});

test('Bug 2: multi-step flow remembers username from step 1 across SPA step 2 and preserves in save payload', async () => {
  const tab = { id: 42, url: 'https://x.com/i/flow/login' };
  const sender = { tab, frameId: 0, url: 'https://x.com/i/flow/login' };

  const hostCalls = [];
  const fakeHost = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === 'match') return { items: [] }; // no existing items initially
    if (cmd === 'save_login') return { item_id: 'vault1:item1' };
    if (cmd === 'update_password') return { ok: true };
    return {};
  };

  // Step 1: user enters username "ReiReieth"
  const step1Res = await B.handle({ type: 'page:capture', kind: 'username', username: 'ReiReieth' }, sender, fakeHost);
  assert.strictEqual(step1Res.prompt, null);

  // Step 2: user enters password only, no username field present on step 2 form
  const step2Res = await B.handle({ type: 'page:capture', kind: 'login', username: '', password: 'secretPassword999' }, sender, fakeHost);
  assert.strictEqual(step2Res.prompt, 'save');
  assert.strictEqual(step2Res.username, 'ReiReieth'); // banner receives ReiReieth!

  // User confirms Save in banner
  const saveRes = await B.handle({
    type: 'page:save_decision',
    action: 'save',
    username: 'ReiReieth',
    user_click: true
  }, sender, fakeHost);

  assert.strictEqual(saveRes.done, true);
  assert.strictEqual(saveRes.action, 'save');

  // Verify that save_login received both origin, username AND password!
  const saveCall = hostCalls.find((c) => c.cmd === 'save_login');
  assert.ok(saveCall, 'save_login must be called');
  assert.strictEqual(saveCall.args.username, 'ReiReieth');
  assert.strictEqual(saveCall.args.password, 'secretPassword999');
  assert.strictEqual(saveCall.args.origin, 'https://x.com');
});

test('Bug 2: offers "update" and saves username when existing item exists for origin without username', async () => {
  const tab = { id: 77, url: 'https://x.com/login' };
  const sender = { tab, frameId: 0, url: 'https://x.com/login' };

  const hostCalls = [];
  // Existing item saved previously with no username (subtitle: '')
  const existingItem = { id: 'v1:itemX', title: 'x.com', subtitle: '', category: 'login' };
  const fakeHost = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === 'match') return { items: [existingItem] };
    if (cmd === 'fill') return { username: '', password: 'oldPassword' };
    if (cmd === 'update_password') return { ok: true };
    return {};
  };

  // Step 1: username entered
  await B.handle({ type: 'page:capture', kind: 'username', username: 'ReiReieth' }, sender, fakeHost);

  // Step 2: password captured
  const captureRes = await B.handle({ type: 'page:capture', kind: 'login', username: 'ReiReieth', password: 'newPassword' }, sender, fakeHost);
  assert.strictEqual(captureRes.prompt, 'update', 'must offer update for existing item');
  assert.strictEqual(captureRes.username, 'ReiReieth');

  // User confirms update
  const decisionRes = await B.handle({
    type: 'page:save_decision',
    action: 'update',
    username: 'ReiReieth',
    user_click: true
  }, sender, fakeHost);

  assert.strictEqual(decisionRes.done, true);
  assert.strictEqual(decisionRes.action, 'update');

  const updateCall = hostCalls.find((c) => c.cmd === 'update_password');
  assert.ok(updateCall, 'update_password called');
  assert.strictEqual(updateCall.args.item_id, 'v1:itemX');
  assert.strictEqual(updateCall.args.password, 'newPassword');
  assert.strictEqual(updateCall.args.username, 'ReiReieth');
});
