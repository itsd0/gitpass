// Tests for Bug 3 & 4: in-page passkey prompt, account name display,
// passthrough when no passkey exists, cancel fallback to native credentials,
// and conditional mediation handling.
const test = require('node:test');
const assert = require('node:assert');
const { JSDOM } = require('jsdom');
const P = require('../lib/passkey-main.js');

test('Bug 4: errorName returns null for not_found and cancelled to allow browser fallback', () => {
  assert.strictEqual(P.errorName('not_found'), null);
  assert.strictEqual(P.errorName('cancelled'), null);
  assert.strictEqual(P.errorName('not_supported'), null);
  assert.strictEqual(P.errorName('no_host'), null);
  assert.strictEqual(P.errorName('exists'), 'InvalidStateError');
  assert.strictEqual(P.errorName('origin_mismatch'), 'SecurityError');
});

test('Bug 4: conditional mediation passes straight through to native credentials', async () => {
  let nativeCalled = false;
  const fakeOrig = async (options) => {
    nativeCalled = true;
    return { id: 'native-cred', type: 'public-key' };
  };

  const options = {
    publicKey: {
      challenge: new Uint8Array([1, 2, 3]),
      rpId: 'x.com'
    },
    mediation: 'conditional'
  };

  const relayFn = async () => {
    throw new Error('relay should not be called for conditional mediation');
  };

  const result = await P.run('get', options, fakeOrig, relayFn);
  assert.strictEqual(nativeCalled, true);
  assert.strictEqual(result.id, 'native-cred');
});

test('Bug 4: when gitpass has no matching passkey (not_found), falls through directly to native credentials with 0 prompt', async () => {
  let nativeCalled = false;
  const fakeOrig = async (options) => {
    nativeCalled = true;
    return { id: 'native-mac-passkey', type: 'public-key' };
  };

  const options = {
    publicKey: {
      challenge: new Uint8Array([1, 2, 3, 4, 5]),
      rpId: 'x.com'
    }
  };

  // Background returns not_found because vault has no passkey for x.com
  const relayFn = async (kind, args) => ({ ok: false, code: 'not_found', error: 'no passkey for x.com' });

  const result = await P.run('get', options, fakeOrig, relayFn);
  assert.strictEqual(nativeCalled, true, 'native credentials.get must be called');
  assert.strictEqual(result.id, 'native-mac-passkey');
});

test('Bug 4: when user cancels gitpass prompt, calls original native credentials', async () => {
  let nativeCalled = false;
  const fakeOrig = async (options) => {
    nativeCalled = true;
    return { id: 'native-touchid-passkey', type: 'public-key' };
  };

  const options = {
    publicKey: {
      challenge: new Uint8Array([1, 2, 3, 4, 5]),
      rpId: 'x.com'
    }
  };

  // Background returns cancelled because user clicked Cancel in gitpass prompt
  const relayFn = async (kind, args) => ({ ok: false, code: 'cancelled', error: 'User cancelled' });

  const result = await P.run('get', options, fakeOrig, relayFn);
  assert.strictEqual(nativeCalled, true, 'native credentials.get must be called after Cancel');
  assert.strictEqual(result.id, 'native-touchid-passkey');
});

test('Bug 4: gesture fallback button triggers native credentials when activation is consumed', async () => {
  const dom = new JSDOM(`<!doctype html><html><body></body></html>`);
  globalThis.document = dom.window.document;

  let nativeCallCount = 0;
  const fakeOrig = async (options) => {
    nativeCallCount++;
    if (nativeCallCount === 1) {
      // First call fails because user activation was consumed during async prompt
      const err = new Error('Failed to execute get on CredentialsContainer: User activation is required');
      err.name = 'NotAllowedError';
      throw err;
    }
    return { id: 'native-after-gesture', type: 'public-key' };
  };

  const options = {
    publicKey: {
      challenge: new Uint8Array([1, 2, 3]),
      rpId: 'x.com'
    }
  };

  const relayFn = async () => ({ ok: false, code: 'cancelled', error: 'User cancelled' });

  const promise = P.run('get', options, fakeOrig, relayFn);

  // Allow DOM insertion of the fallback gesture button
  await new Promise((r) => setTimeout(r, 20));

  const continueBtn = dom.window.document.querySelector('button');
  assert.ok(continueBtn, 'gesture fallback button must be rendered');
  assert.strictEqual(continueBtn.textContent, 'Continue');

  // User clicks the fallback button with fresh gesture
  continueBtn.click();

  const result = await promise;
  assert.strictEqual(result.id, 'native-after-gesture');
  assert.strictEqual(nativeCallCount, 2);

  delete globalThis.document;
  dom.window.close();
});
