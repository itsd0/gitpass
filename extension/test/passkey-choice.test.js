// Tests for issue #241: passkey overlay account choice in bg-passkey.js.
// Unit tests for the background: several matches put the chosen credential_id in the host call,
// and an unknown credential_id is refused with bad_request.
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

function loadBg(tabReplies = []) {
  const sent = [];
  const chrome = {
    runtime: { id: "x", lastError: undefined },
    tabs: {
      sendMessage: (tabId, msg, opts, cb) => {
        sent.push({ tabId, msg, opts });
        const r = tabReplies.shift();
        setTimeout(() => cb(r), 0);
      }
    },
    windows: {
      create: async () => ({ id: 99 }),
      onRemoved: { addListener() {} }
    }
  };
  const ctx = {
    chrome,
    console,
    URL,
    setTimeout,
    clearTimeout,
    crypto: globalThis.crypto,
    GitpassPolicy: {
      originOf: (u) => new URL(u).origin,
      isFillableOrigin: (o) => /^https?:/.test(o)
    }
  };
  ctx.globalThis = ctx;
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "..", "bg-passkey.js"), "utf8"), ctx);
  return { P: ctx.GitpassPasskey, sent, reqs: ctx.GitpassPasskey._reqs };
}

test("fromPage: several matches put the chosen credential_id in the host call", async () => {
  const matching = [
    { credential_id: "cred-alice", user_name: "alice@example.com", vault_name: "Personal", last_used: 100 },
    { credential_id: "cred-bob", user_name: "bob@example.com", vault_name: "Work", last_used: 500 }
  ];
  // Tab chooses the second credential (bob)
  const { P, sent } = loadBg([{ approved: true, credential_id: "cred-bob" }]);
  const hostCalls = [];
  const host = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === "passkey_query") return { rp_id: "example.com", matching };
    if (cmd === "passkey_get") return { credential_id: args.credential_id };
    return {};
  };

  const sender = { tab: { id: 42 }, frameId: 0, origin: "https://example.com" };
  const res = await P.fromPage({ type: "passkey:get", args: { challenge: "Y2hhbA" } }, sender, host);

  assert.strictEqual(res.credential_id, "cred-bob");
  assert.strictEqual(sent.length, 2);
  assert.strictEqual(sent[1].msg.type, "gitpass-passkey-result");
  assert.strictEqual(sent[1].msg.ok, true);
  assert.strictEqual(sent[0].msg.type, "gitpass-passkey-prompt");
  assert.strictEqual(sent[0].msg.kind, "get");
  assert.strictEqual(sent[0].msg.matching.length, 2);
  // Matches sorted newest last_used first: cred-bob first, then cred-alice
  assert.strictEqual(sent[0].msg.matching[0].credential_id, "cred-bob");
  assert.strictEqual(sent[0].msg.matching[1].credential_id, "cred-alice");

  const getCall = hostCalls.find((c) => c.cmd === "passkey_get");
  assert.ok(getCall, "passkey_get was called");
  assert.strictEqual(getCall.args.credential_id, "cred-bob");
  assert.strictEqual(getCall.args.approved, true);
  assert.strictEqual(getCall.args.source, "popup");
});

test("fromPage: unknown credential_id returned from prompt is rejected with bad_request", async () => {
  const matching = [
    { credential_id: "cred-alice", user_name: "alice@example.com", vault_name: "Personal", last_used: 100 }
  ];
  // Tab returns a forged / unknown credential_id
  const { P } = loadBg([{ approved: true, credential_id: "cred-forged-evil" }]);
  const hostCalls = [];
  const host = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === "passkey_query") return { rp_id: "example.com", matching };
    if (cmd === "passkey_get") return { credential_id: args.credential_id };
    return {};
  };

  const sender = { tab: { id: 42 }, frameId: 0, origin: "https://example.com" };
  await assert.rejects(
    P.fromPage({ type: "passkey:get", args: { challenge: "Y2hhbA" } }, sender, host),
    (err) => {
      assert.strictEqual(err.code, "bad_request");
      assert.match(err.message, /Invalid credential_id/);
      return true;
    }
  );

  assert.strictEqual(hostCalls.filter((c) => c.cmd === "passkey_get").length, 0, "passkey_get must not be called");
});

test("fromPage: prompt approval without explicit credential_id defaults to first match", async () => {
  const matching = [
    { credential_id: "cred-alice", user_name: "alice@example.com", vault_name: "Personal", last_used: 200 }
  ];
  const { P } = loadBg([{ approved: true }]);
  const hostCalls = [];
  const host = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === "passkey_query") return { rp_id: "example.com", matching };
    if (cmd === "passkey_get") return { credential_id: args.credential_id };
    return {};
  };

  const sender = { tab: { id: 42 }, frameId: 0, origin: "https://example.com" };
  const res = await P.fromPage({ type: "passkey:get", args: { challenge: "Y2hhbA" } }, sender, host);
  assert.strictEqual(res.credential_id, "cred-alice");
  const getCall = hostCalls.find((c) => c.cmd === "passkey_get");
  assert.strictEqual(getCall.args.credential_id, "cred-alice");
});

test("fromUi: fallback popup window receives matching choices and validates chosen credential", async () => {
  const { P, reqs } = loadBg([]);
  const matching = [
    { credential_id: "cred-1", user_name: "user1@site.com", vault_name: "V1", last_used: 10 },
    { credential_id: "cred-2", user_name: "user2@site.com", vault_name: "V2", last_used: 20 }
  ];

  let resolved = null;
  let rejected = null;
  const p = new Promise((res, rej) => { resolved = res; rejected = rej; });
  reqs.set("req-test-1", {
    kind: "get",
    origin: "https://site.com",
    args: { rp_id: "site.com", challenge: "Y2hhbA" },
    tabId: 10,
    resolve: resolved,
    reject: rejected,
    user_name: "user2@site.com",
    matching
  });

  const hostCalls = [];
  const host = async (cmd, args) => {
    hostCalls.push({ cmd, args });
    if (cmd === "passkey_get") return { credential_id: args.credential_id, ok: true };
    return {};
  };

  // 1. passkey-ui:request returns matching array
  const reqData = await P.fromUi({ type: "passkey-ui:request", req: "req-test-1" }, host);
  assert.strictEqual(reqData.kind, "get");
  assert.strictEqual(reqData.matching.length, 2);
  assert.strictEqual(reqData.matching[0].credential_id, "cred-1");
  assert.strictEqual(reqData.matching[1].credential_id, "cred-2");

  // 2. passkey-ui:decide with unknown credential_id returns bad_request
  const badDecide = await P.fromUi({ type: "passkey-ui:decide", req: "req-test-1", approved: true, credentialId: "cred-evil" }, host);
  assert.strictEqual(badDecide.code, "bad_request");
  await assert.rejects(p, (e) => e.code === "bad_request");

  // 3. New request with valid selection
  let resolved2 = null;
  const p2 = new Promise((res, rej) => { resolved2 = res; });
  reqs.set("req-test-2", {
    kind: "get",
    origin: "https://site.com",
    args: { rp_id: "site.com", challenge: "Y2hhbA" },
    tabId: 10,
    resolve: resolved2,
    reject: () => {},
    user_name: "user2@site.com",
    matching
  });

  const okDecide = await P.fromUi({ type: "passkey-ui:decide", req: "req-test-2", approved: true, credentialId: "cred-2" }, host);
  assert.strictEqual(okDecide.done, true);
  const getCall = hostCalls.find((c) => c.cmd === "passkey_get");
  assert.strictEqual(getCall.args.credential_id, "cred-2");
  const res2 = await p2;
  assert.strictEqual(res2.credential_id, "cred-2");
});
