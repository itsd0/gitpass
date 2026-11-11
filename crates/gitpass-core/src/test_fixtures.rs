//! Shared encrypted cross-platform test vault fixtures (issue #451).
//!
//! Test-support only: builds the committed fixtures under
//! `crates/gitpass-core/tests/fixtures/shared/` — one vault directory per
//! format version (v2, v3, v4, v5) — plus an `expected.json` next to each
//! one holding the decrypted content every consumer must reproduce.
//!
//! The four fixtures carry the *same* item content (every category, a
//! trashed item, an archived item, a favorite, TOTP, password history,
//! item history, a passkey item, generator history, two vaults). They
//! differ in what their era supports:
//! - v2/v3: no identity, no attachments (manifest v3, devices only).
//! - v4: an account identity keypair (manifest v4).
//! - v5: an attachment on the example login (manifest v5).
//!   The format version is the `format.json` version; the manifest format
//!   is content-derived (`store::manifest_format_for`), as everywhere else.
//!
//! Credentials are throwaway test-only values: the password is
//! [`FIXTURE_PASSWORD`] and the Secret Key is written to `secret.key.txt`
//! next to each fixture. All item content is synthetic (`FAKE-…`,
//! `example.invalid`).
//!
//! Two timestamps are deliberately set far in the future
//! ([`FAR_FUTURE_SECS`], 2100-01-01) so the fixtures never rot: the
//! trashed item's `deleted_at` (a normal unlock purges soft-deleted items
//! older than 30 days) and the generator-history entries' `created_at`
//! (entries older than 90 days are pruned on read). The expected JSON
//! never contains those timestamps — the semantic dump (see
//! [`semantic_record`]) drops every volatile field — so the sentinel
//! values are an implementation detail of the fixture, not of the
//! expectation.
//!
//! How v3/v4 are produced: current code only ever writes format v2
//! (`store::init`) or v5 (`Session::rewrap_keyset`), so a v3/v4
//! `format.json` is produced by [`rewrite_format_version`], which
//! re-wraps the keyset under a `Format` with the requested version using
//! only public core crypto primitives, then re-seals the manifest with
//! the new keyset hash (preserving its format). That mirrors what the
//! v0.3/v0.4 releases did when their `FORMAT_VERSION` was 3/4.
//!
//! Tests must copy a fixture to a temp dir before opening it: a normal
//! unlock writes device state, may re-wrap the keyset to the production
//! KDF, and purges expired trash. [`dump_fixture`] opens read-only
//! (`Session::unlock_readonly`), which never mutates the vault.

use crate::crypto::{
    derive_auk, seal, unwrap_key, wrap_key, Envelope, KdfParams, SecretKey,
};
use crate::gen_history::{GeneratorHistory, GeneratorHistoryEntry, GEN_HISTORY_FORMAT};
use crate::item::{Category, Field, FieldKind, Section};
use crate::merge::ItemRecord;
use crate::store::{
    self, hash_bytes, keyset_aad, manifest_key, Format, Item, Manifest, Session, UnlockOpts,
    FORMAT_VERSION, MIN_READ_FORMAT,
};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Master password for every shared fixture. Test-only, documented in the
/// fixtures README; never a real credential.
pub const FIXTURE_PASSWORD: &str = "fixture-password-451";

/// Fixed Secret Key bytes for every shared fixture (encoded form is
/// written to each fixture's `secret.key.txt`). Test-only.
pub const FIXTURE_SECRET_KEY_BYTES: [u8; 16] = [0x51; 16];

/// Sentinel timestamp (2100-01-01T00:00:00Z) for `deleted_at` and
/// generator-history `created_at`: far enough in the future that the
/// 30-day trash purge and the 90-day history prune never fire on a
/// static fixture, no matter when the test runs.
pub const FAR_FUTURE_SECS: u64 = 4_102_444_800;

/// Fixture format versions, in order.
pub const VERSIONS: [u32; 4] = [2, 3, 4, 5];

/// The fixed Secret Key shared by all fixtures.
pub fn fixture_secret_key() -> SecretKey {
    SecretKey(FIXTURE_SECRET_KEY_BYTES)
}

fn fast_kdf() -> KdfParams {
    KdfParams { alg: "argon2id".into(), m_kib: 8192, t: 1, p: 1 }
}

fn opts() -> UnlockOpts {
    UnlockOpts { accept_rollback: false, target_kdf: fast_kdf() }
}

/// Directory name for a fixture version (`v2`, …).
pub fn dir_name(version: u32) -> String {
    format!("v{version}")
}

/// Manifest format a freshly built fixture of `version` must have
/// (content-derived: devices only → v3, identity → v4, attachments → v5).
fn expected_manifest_format(version: u32) -> u32 {
    match version {
        2 | 3 => 3,
        4 => 4,
        5 => 5,
        _ => unreachable!(),
    }
}

// ---------- record builders (fixed ids, fake content) ----------

/// Fixed item ids, so `related_item_ids` and the expected JSON are
/// stable across regenerations. Valid UUIDv4 strings.
pub mod ids {
    pub const LOGIN: &str = "11111111-1111-4111-8111-111111111111";
    pub const ROTATED: &str = "22222222-2222-4222-8222-222222222222";
    pub const CARD: &str = "33333333-3333-4333-8333-333333333333";
    pub const NOTE: &str = "44444444-4444-4444-8444-444444444444";
    pub const IDENTITY: &str = "55555555-5555-4555-8555-555555555555";
    pub const API: &str = "66666666-6666-4666-8666-666666666666";
    pub const DOCUMENT: &str = "77777777-7777-4777-8777-777777777777";
    pub const SSH: &str = "88888888-8888-4888-8888-888888888888";
    pub const WIFI: &str = "99999999-9999-4999-8999-999999999999";
    pub const PASSKEY: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    pub const BANK: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    pub const SERVER: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    pub const ARCHIVED: &str = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    pub const TRASHED: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
    pub const EDITED: &str = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    pub const IGNORED: &str = "12345678-1234-4123-8123-123456789012";
    pub const WORK_LOGIN: &str = "11112222-1111-4111-8111-111122221111";
    pub const WORK_NOTE: &str = "33334444-3333-4333-8333-333344443333";
}

fn field(id: &str, kind: FieldKind, value: &str) -> Field {
    Field { id: id.into(), label: id.into(), kind, value: Some(value.into()), removed: false }
}

fn section(id: &str, fields: Vec<Field>) -> Section {
    Section { id: id.into(), title: Some(id.into()), fields }
}

/// A record whose `sections` are stored verbatim; for login items the
/// `login` section folds into the legacy username/password/totp slots
/// (see `ItemRecord::apply_sections`). `url`/`notes` are set afterwards
/// — `apply_sections` leaves them alone.
fn record(cat: Category, title: &str, sections: Vec<Section>) -> ItemRecord {
    let mut item = Item::default();
    item.title = title.into();
    // Fixed `updated_at` keeps the payload stable; the semantic dump
    // strips timestamps anyway, this is only for realism.
    item.updated_at = "1700000000".into();
    let mut rec = ItemRecord::new(item);
    rec.category = cat;
    rec.apply_sections(&sections);
    rec
}

fn login(title: &str, username: &str, password: &str) -> ItemRecord {
    use FieldKind::*;
    record(
        Category::Login,
        title,
        vec![section("login", vec![
            field("username", Text, username),
            field("password", Concealed, password),
        ])],
    )
}

/// The full item set for the "Personal" vault: one item per category in
/// [`Category::ALL`], plus the flagged/trashed/history cases. Returns
/// `(item id, record)` pairs in a stable order.
fn personal_items() -> Vec<(&'static str, ItemRecord)> {
    use FieldKind::*;
    let mut out: Vec<(&str, ItemRecord)> = vec![];

    let mut login_rec = record(
        Category::Login,
        "Example Login",
        vec![
            section("login", vec![
                field("username", Text, "alice@example.invalid"),
                field("password", Concealed, "FAKE-login-password-1"),
                field("totp", Totp, "JBSWY3DPEHPK3PXP"),
            ]),
            section("recovery", vec![
                field("recovery-code", Concealed, "FAKE-recovery-123456"),
                field("security-question", Text, "Fake answer"),
            ]),
        ],
    );
    login_rec.item.url = Some("https://example.invalid/login".into());
    login_rec.item.notes = Some("Fake login note".into());
    login_rec.tags = vec!["personal".into(), "2fa".into()];
    login_rec.favorite = true;
    login_rec.rotate_every_days = Some(90);
    login_rec.related_item_ids = vec![ids::API.into()];
    out.push((ids::LOGIN, login_rec));

    out.push((ids::ROTATED, login("Rotated Login", "bob@example.invalid", "FAKE-rotated-password-1")));

    out.push((ids::CARD, record(Category::CreditCard, "Example Visa", vec![section("card", vec![
        field("number", Concealed, "4111111111111111"),
        field("expiry", MonthYear, "12/30"),
        field("cvv", Concealed, "123"),
        field("holder", Text, "Test Person"),
    ])])));

    out.push((ids::NOTE, record(Category::SecureNote, "Example Note", vec![section("note", vec![
        field("body", Multiline, "line1\nline2\nFake note body"),
    ])])));

    out.push((ids::IDENTITY, record(Category::Identity, "Example Person", vec![
        section("name", vec![
            field("first", Text, "Test"),
            field("last", Text, "Person"),
            field("birth", Date, "2000-01-01"),
        ]),
        section("contact", vec![
            field("email", Email, "test@example.invalid"),
            field("phone", Phone, "+1 555 0100"),
        ]),
    ])));

    out.push((ids::API, record(Category::ApiCredential, "Example API", vec![section("api", vec![
        field("key", Concealed, "FAKE-api-key-1"),
        field("host", Url, "https://api.example.invalid"),
    ])])));

    out.push((ids::DOCUMENT, record(Category::Document, "Example Document", vec![section("doc", vec![
        field("desc", Text, "Fake scan of a document"),
    ])])));

    out.push((ids::SSH, record(Category::SshKey, "Example SSH", vec![section("main", vec![
        field("private", Concealed, "-----BEGIN FAKE OPENSSH PRIVATE KEY-----\nFAKE\n-----END FAKE OPENSSH PRIVATE KEY-----"),
        field("public", Text, "ssh-ed25519 AAAAFAKE example.invalid"),
    ])])));

    out.push((ids::WIFI, record(Category::Wifi, "Example Wifi", vec![section("wifi", vec![
        field("ssid", Text, "FakeNet"),
        field("wifi_password", Concealed, "FAKE-wifi-password"),
    ])])));

    out.push((ids::PASSKEY, record(Category::Passkey, "Example Passkey", vec![section("main", vec![
        field("rp_id", Text, "example.invalid"),
        field("credential_id", Text, "AAAAFAKECREDENTIALID"),
        field("private_key", Concealed, "FAKE-passkey-private-key"),
    ])])));

    out.push((ids::BANK, record(Category::BankAccount, "Example Bank", vec![section("main", vec![
        field("bank_name", Text, "Example Bank"),
        field("account_number", Concealed, "1234567890"),
        field("pin", Concealed, "1234"),
    ])])));

    out.push((ids::SERVER, record(Category::Server, "Example Server", vec![section("main", vec![
        field("url", Url, "https://srv.example.invalid"),
        field("username", Text, "root"),
        field("password", Concealed, "FAKE-server-password"),
    ])])));

    let mut archived = login("Archived Login", "archived@example.invalid", "FAKE-archived-password");
    archived.archived = true;
    out.push((ids::ARCHIVED, archived));

    let trashed = login("Trashed Login", "trashed@example.invalid", "FAKE-trashed-password");
    out.push((ids::TRASHED, trashed));

    let mut edited = login("Edited Login", "edited@example.invalid", "FAKE-edited-v1");
    edited.item.notes = Some("First version note".into());
    out.push((ids::EDITED, edited));

    let mut ignored = login("Ignored Finding Login", "ignored@example.invalid", "FAKE-ignored-password");
    ignored.ignored_findings = vec!["reused".into()];
    out.push((ids::IGNORED, ignored));

    out
}

fn work_items() -> Vec<(&'static str, ItemRecord)> {
    use FieldKind::*;
    let mut work_login = login("Work Login", "workuser@example.invalid", "FAKE-work-password");
    work_login.item.url = Some("https://work.example.invalid".into());
    vec![
        (ids::WORK_LOGIN, work_login),
        (ids::WORK_NOTE, record(Category::SecureNote, "Work Note", vec![section("note", vec![
            field("body", Multiline, "Fake work note"),
        ])])),
    ]
}

// ---------- semantic dump (the expected JSON) ----------

/// Recursively drop every volatile field from a serialized `ItemRecord`:
/// wall-clock timestamps (`updated_at`, `created_at`, `replaced_at`),
/// merge clocks (`clocks`, `xclocks`) and random attachment ids.
/// `deleted_at` becomes an explicit `deleted` boolean. Everything else
/// (titles, values, flags, history snapshots, password history values,
/// attachment names/sizes) is compared verbatim.
fn strip_volatile(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("updated_at");
            m.remove("created_at");
            m.remove("clocks");
            m.remove("xclocks");
            m.remove("replaced_at");
            if let Some(d) = m.remove("deleted_at") {
                m.insert("deleted".into(), json!(!d.is_null()));
            }
            if let Some(Value::Array(atts)) = m.get_mut("attachments") {
                for a in atts.iter_mut() {
                    if let Value::Object(am) = a {
                        am.remove("id");
                    }
                }
            }
            for (_, v) in m.iter_mut() {
                strip_volatile(v);
            }
        }
        Value::Array(a) => {
            for v in a.iter_mut() {
                strip_volatile(v);
            }
        }
        _ => {}
    }
}

/// The semantic form of one decrypted record, for comparison against
/// `expected.json`. Shared by the core test, the bridge test and (via
/// the committed JSON) the Android/iOS tests.
pub fn semantic_record(rec: &ItemRecord) -> Value {
    let mut v = serde_json::to_value(rec).expect("ItemRecord serializes");
    strip_volatile(&mut v);
    if let Value::Object(m) = &mut v {
        m.entry("deleted").or_insert(json!(false));
    }
    v
}

/// One vault's semantic JSON. `icon`/`color` are included only when set.
pub fn vault_json(
    name: &str,
    description: Option<&str>,
    icon: Option<&str>,
    color: Option<&str>,
    mut items: Vec<Value>,
) -> Value {
    items.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    let mut v = json!({"name": name, "description": description, "items": items});
    if let Some(icon) = icon {
        v["icon"] = json!(icon);
    }
    if let Some(color) = color {
        v["color"] = json!(color);
    }
    v
}

/// Decrypt a fixture (read-only unlock) and return its semantic content:
/// `{format_version, vaults: [...], generator_history: [...]}`.
/// Generator history is reduced to value/source/host (ids and timestamps
/// are volatile). Vaults and items are sorted by name/title.
pub fn dump_fixture(root: &Path, password: &str, secret: &SecretKey) -> Result<Value> {
    let fmt = store::read_format(root)?;
    let s = Session::unlock_readonly(root, password, secret)?;
    let mut vaults = vec![];
    for (vid, meta) in s.list_vaults()? {
        let mut items = vec![];
        for rec in s.list_records(&vid)?.into_iter().map(|(_, _, r)| r) {
            items.push(semantic_record(&rec));
        }
        vaults.push(vault_json(
            &meta.name,
            meta.description.as_deref(),
            meta.icon.as_deref(),
            meta.color.as_deref(),
            items,
        ));
    }
    vaults.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let gh = s.read_generator_history()?;
    let gen: Vec<Value> = gh
        .entries
        .iter()
        .map(|e| json!({"value": e.value, "source": e.source, "host": e.host}))
        .collect();
    Ok(json!({"format_version": fmt.version, "vaults": vaults, "generator_history": gen}))
}

// ---------- format-version rewrite (v3/v4) ----------

/// Re-wrap the keyset under a `Format` with the requested version and
/// re-seal the manifest with the new keyset hash, preserving the
/// manifest's format and contents. Only public core crypto primitives
/// are used; the manifest AAD (`"{account_id}|manifest|2"`) mirrors
/// `store::format::manifest_aad`, which is crate-private.
///
/// The account key itself never changes, so vault keys, items and the
/// manifest payload stay valid — only `format.json`, `keyset.enc` and
/// the manifest's `keyset_hash` change.
pub fn rewrite_format_version(
    root: &Path,
    password: &str,
    secret: &SecretKey,
    version: u32,
) -> Result<()> {
    if !(MIN_READ_FORMAT..=FORMAT_VERSION).contains(&version) {
        bail!("cannot rewrite to unsupported format version {version}");
    }
    let fmt = store::read_format(root)?;
    let salt = B64.decode(&fmt.salt)?;
    let auk = derive_auk(password, secret, &salt, &fmt.account_id, &fmt.kdf)?;
    let env: Envelope = serde_json::from_slice(&fs::read(root.join("keyset.enc"))?)?;
    let ak = unwrap_key(&auk, &env, &keyset_aad(&fmt))?;
    let nf = Format { version, ..fmt };
    let nenv = wrap_key(&auk, &ak, &keyset_aad(&nf))?;
    fs::write(root.join("keyset.enc"), serde_json::to_vec_pretty(&nenv)?)?;
    fs::write(root.join("format.json"), serde_json::to_vec_pretty(&nf)?)?;
    // Re-seal the manifest with the new keyset hash. A normal unlock
    // would rebuild it with FORMAT_VERSION instead, which would lie
    // about the manifest era, so this is done by hand.
    let s = Session::unlock_readonly(root, password, secret)?;
    let mut m: Manifest = s.load_manifest()?;
    m.keyset_hash = hash_bytes(&fs::read(root.join("keyset.enc"))?);
    let aad = format!("{}|manifest|2", nf.account_id);
    let menv = seal(&manifest_key(&ak), &serde_json::to_vec(&m)?, aad.as_bytes())?;
    fs::write(root.join("manifest.enc"), serde_json::to_vec_pretty(&menv)?)?;
    Ok(())
}

// ---------- fixture construction ----------

/// Device-local files a committed fixture must not carry: a test that
/// opens the fixture gets a fresh device id / epoch / throttle state.
fn strip_local_state(root: &Path) {
    for n in [
        ".gitpass.lock",
        ".gitpass-device.json",
        ".gitpass-device-key.json",
        ".gitpass-state.json",
        ".gitpass-throttle.json",
    ] {
        let _ = fs::remove_file(root.join(n));
    }
}

/// Build one fixture (encrypted vault + `expected.json`) in `out_dir`,
/// which must not exist yet. Returns the semantic dump that was written
/// to `expected.json`.
pub fn build_fixture(version: u32, out_dir: &Path) -> Result<Value> {
    if !VERSIONS.contains(&version) {
        bail!("unsupported fixture version {version}");
    }
    if out_dir.exists() {
        bail!("fixture dir {} already exists", out_dir.display());
    }
    fs::create_dir_all(out_dir)?;
    let secret = fixture_secret_key();
    // init always writes format v2 with a fresh account; the fixed
    // secret key is used for every unlock from here on.
    let init_secret = store::init(out_dir, FIXTURE_PASSWORD, fast_kdf())?;
    // Re-bind the keyset to the FIXED test secret (init generated a
    // random one). For v5 this is the real rewrap path; for v2/v3/v4 the
    // format version is corrected afterwards (see below).
    let mut s = Session::unlock_device(
        out_dir,
        FIXTURE_PASSWORD,
        &init_secret,
        &opts(),
        Some("shared-fixture"),
    )?;
    s.rewrap_keyset(FIXTURE_PASSWORD, &secret, fast_kdf())?;

    let personal = s.create_vault_full(
        "Personal",
        Some("Fake personal vault for cross-platform fixture tests".into()),
        None,
        None,
    )?;
    let work = s.create_vault_full(
        "Work",
        Some("Fake work vault for cross-platform fixture tests".into()),
        None,
        None,
    )?;
    for (iid, rec) in personal_items() {
        s.put_record(&personal, iid, &rec)?;
    }
    for (iid, rec) in work_items() {
        s.put_record(&work, iid, &rec)?;
    }

    // Rotated Login: change the password once through the real update
    // path, so the item gains one history entry and a password-history
    // entry with the old password.
    let (_, rotated) = s.read_record(&personal, ids::ROTATED)?;
    let mut rotated2 = rotated.clone();
    rotated2.item.password = Some("FAKE-rotated-password-2".into());
    s.update_record(&personal, ids::ROTATED, rotated2)?;

    // Edited Login: two content edits → two history entries.
    for (pw, notes) in [
        ("FAKE-edited-v2", "Second version note"),
        ("FAKE-edited-v3", "Third version note"),
    ] {
        let (_, cur) = s.read_record(&personal, ids::EDITED)?;
        let mut next = cur.clone();
        next.item.password = Some(pw.into());
        next.item.notes = Some(notes.into());
        s.update_record(&personal, ids::EDITED, next)?;
    }

    // Trashed Login: soft-delete, then pin `deleted_at` to the far-future
    // sentinel so the 30-day purge can never remove it from a static
    // fixture. The dump reports this as `"deleted": true`.
    s.soft_delete(&personal, ids::TRASHED)?;
    let (_, mut trashed) = s.read_record(&personal, ids::TRASHED)?;
    trashed.deleted_at = Some(FAR_FUTURE_SECS);
    s.put_record(&personal, ids::TRASHED, &trashed)?;

    // Generator history with fixed ids and far-future timestamps, so the
    // 90-day prune never drops the entries from a static fixture.
    let gh = GeneratorHistory {
        format: GEN_HISTORY_FORMAT,
        entries: vec![
            GeneratorHistoryEntry {
                id: "10000000-0000-4000-8000-000000000001".into(),
                value: "FAKE-generated-password-1".into(),
                created_at: FAR_FUTURE_SECS,
                source: "generator".into(),
                host: Some("example.invalid".into()),
            },
            GeneratorHistoryEntry {
                id: "10000000-0000-4000-8000-000000000002".into(),
                value: "FAKE-generated-password-2".into(),
                created_at: FAR_FUTURE_SECS,
                source: "cli".into(),
                host: None,
            },
        ],
        cleared_at: None,
        deleted: vec![],
    };
    s.write_generator_history(&gh)?;

    if version >= 4 {
        // Account identity: makes the manifest v4.
        s.ensure_identity()?;
    }
    if version == 5 {
        // Attachment: makes the manifest v5. Adding it bumps the login
        // item's history (content change); that is genuine behaviour.
        s.add_attachment(
            &personal,
            ids::LOGIN,
            "fake-note.txt",
            "text/plain",
            b"FAKE attachment content\n",
        )?;
    }

    // Settle the format.json version. v5 is already there (rewrap);
    // v2 must be restored (the rewrap bumped it to 5); v3/v4 are
    // rewritten to their historical version.
    match version {
        5 => {}
        2..=4 => rewrite_format_version(out_dir, FIXTURE_PASSWORD, &secret, version)?,
        _ => unreachable!(),
    }

    // The fixture must open cleanly and carry the manifest format its
    // era implies; anything else means the construction above drifted.
    let check = Session::unlock_readonly(out_dir, FIXTURE_PASSWORD, &secret)?;
    let m = check.load_manifest()?;
    if m.format != expected_manifest_format(version) {
        bail!(
            "v{version} fixture has manifest format {}, expected {}",
            m.format,
            expected_manifest_format(version)
        );
    }
    drop(check);

    // The fixed Secret Key is the documented test-only credential.
    fs::write(out_dir.join("secret.key.txt"), format!("{}\n", secret.encode()))?;
    strip_local_state(out_dir);
    let expected = dump_fixture(out_dir, FIXTURE_PASSWORD, &secret)?;
    fs::write(
        out_dir.join("expected.json"),
        serde_json::to_string_pretty(&expected)?,
    )?;
    // The dump may have recreated device-local state (throttle/device
    // id); strip it again so the committed fixture is pristine.
    strip_local_state(out_dir);
    Ok(expected)
}

/// Build all four fixtures under `out_dir` (one `vN` directory each,
/// created fresh; pre-existing `vN` directories are removed first).
pub fn generate_all(out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)?;
    for v in VERSIONS {
        let d = out_dir.join(dir_name(v));
        if d.exists() {
            fs::remove_dir_all(&d)?;
        }
        build_fixture(v, &d)?;
    }
    Ok(())
}

/// Regenerate every fixture into a temp dir and compare each generated
/// `expected.json` with the committed one under `committed_dir`.
/// Fails on the first mismatch, naming the version and the differing
/// field path. Encrypted bytes are intentionally not compared: salts,
/// nonces and account ids are random per generation, only the decrypted
/// content is reproducible.
pub fn check_all(committed_dir: &Path) -> Result<()> {
    let tmp = tempfile::tempdir()?;
    generate_all(tmp.path())?;
    for v in VERSIONS {
        let name = dir_name(v);
        let want_path = committed_dir.join(&name).join("expected.json");
        let want: Value = serde_json::from_str(
            &fs::read_to_string(&want_path)
                .with_context(|| format!("reading {}", want_path.display()))?,
        )?;
        let got_path = tmp.path().join(&name).join("expected.json");
        let got: Value = serde_json::from_str(&fs::read_to_string(&got_path)?)?;
        if want != got {
            bail!(
                "generator drift in {name}/expected.json: regenerated content differs from committed.\ncommitted: {}\nregenerated: {}",
                serde_json::to_string_pretty(&want)?,
                serde_json::to_string_pretty(&got)?
            );
        }
    }
    Ok(())
}

/// Read a fixture's Secret Key from its `secret.key.txt`.
pub fn read_secret_key(fixture_dir: &Path) -> Result<SecretKey> {
    let s = fs::read_to_string(fixture_dir.join("secret.key.txt"))?;
    SecretKey::parse(s.trim())
}

/// Copy a fixture tree to `dest` (tests open the copy, never the
/// committed fixture: unlocking mutates device state and may re-wrap
/// the keyset).
pub fn copy_fixture(src: &Path, dest: &Path) -> Result<()> {
    copy_dir(src, dest)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let t = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            fs::copy(e.path(), t)?;
        }
    }
    Ok(())
}

/// Test-only: locate the shared fixtures directory for a crate whose
/// `CARGO_MANIFEST_DIR` is `manifest_dir`.
pub fn shared_fixtures_dir(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("tests").join("fixtures").join("shared")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_secret_key_round_trips() {
        let sk = fixture_secret_key();
        let enc = sk.encode();
        assert!(enc.starts_with("A3-"), "{enc}");
        assert_eq!(SecretKey::parse(&enc).unwrap().0, FIXTURE_SECRET_KEY_BYTES);
    }
}
