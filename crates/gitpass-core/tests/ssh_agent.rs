//! Core SSH agent integration tests with a throwaway vault.
//! Tests `session_identities`, `StaticKeys`, and `serve_stream` over a Unix stream pair.
//! No external tools or real SSH files used.
use gitpass_core::crypto::KdfParams;
use gitpass_core::ssh::{self, *};
use gitpass_core::store::{self, UnlockOpts};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

fn fast() -> KdfParams {
    KdfParams {
        alg: "argon2id".into(),
        m_kib: 8192,
        t: 1,
        p: 1,
    }
}

fn request_identities(s: &mut UnixStream) -> Vec<(Vec<u8>, String)> {
    let req = [0u8, 0, 0, 1, SSH_AGENTC_REQUEST_IDENTITIES];
    s.write_all(&req).unwrap();
    s.flush().unwrap();
    let mut len_b = [0u8; 4];
    s.read_exact(&mut len_b).unwrap();
    let len = u32::from_be_bytes(len_b) as usize;
    let mut resp = vec![0u8; len];
    s.read_exact(&mut resp).unwrap();
    assert_eq!(resp[0], SSH_AGENT_IDENTITIES_ANSWER);
    let n = u32::from_be_bytes(resp[1..5].try_into().unwrap()) as usize;
    let mut at = 5;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let blen = u32::from_be_bytes(resp[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        let blob = resp[at..at + blen].to_vec();
        at += blen;
        let clen = u32::from_be_bytes(resp[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        let comment = String::from_utf8(resp[at..at + clen].to_vec()).unwrap();
        at += clen;
        out.push((blob, comment));
    }
    out
}

fn sign_request(s: &mut UnixStream, blob: &[u8], data: &[u8]) -> Result<Vec<u8>, u8> {
    let mut body = vec![SSH_AGENTC_SIGN_REQUEST];
    body.extend_from_slice(&(blob.len() as u32).to_be_bytes());
    body.extend_from_slice(blob);
    body.extend_from_slice(&(data.len() as u32).to_be_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(&0u32.to_be_bytes());

    let len = (body.len() as u32).to_be_bytes();
    s.write_all(&len).unwrap();
    s.write_all(&body).unwrap();
    s.flush().unwrap();

    let mut len_b = [0u8; 4];
    s.read_exact(&mut len_b).unwrap();
    let resp_len = u32::from_be_bytes(len_b) as usize;
    let mut resp = vec![0u8; resp_len];
    s.read_exact(&mut resp).unwrap();

    if resp[0] == SSH_AGENT_SIGN_RESPONSE {
        let sig_len = u32::from_be_bytes(resp[1..5].try_into().unwrap()) as usize;
        Ok(resp[5..5 + sig_len].to_vec())
    } else {
        Err(resp[0])
    }
}

#[test]
fn serve_stream_over_unix_pair_with_temp_vault_keys() {
    let d = tempfile::tempdir().unwrap();
    let sk = store::init(d.path(), "correct horse", fast()).unwrap();
    let opts = UnlockOpts {
        accept_rollback: false,
        target_kdf: fast(),
    };
    let s = store::Session::unlock_device(d.path(), "correct horse", &sk, &opts, Some("test"))
        .unwrap();

    let vault_id = s.create_vault("Dev").unwrap();

    // 1. Initial state: 0 identities
    let ids = session_identities(&s).unwrap();
    assert_eq!(ids.len(), 0);

    // 2. Add Ed25519 key to vault
    let k1 = SshKey::generate_ed25519("key1@example.invalid").unwrap();
    let rec1 = k1.to_record("Work Key").unwrap();
    let iid1 = s.add_record(&vault_id, rec1).unwrap();

    // 3. Add second key to vault
    let k2 = SshKey::generate_ed25519("key2@example.invalid").unwrap();
    let rec2 = k2.to_record("Personal Key").unwrap();
    let iid2 = s.add_record(&vault_id, rec2).unwrap();

    // Check session identities
    let ids = session_identities(&s).unwrap();
    assert_eq!(ids.len(), 2);

    // 4. Serve over UnixStream pair
    let (mut server, mut client) = UnixStream::pair().unwrap();
    let provider = Arc::new(StaticKeys {
        ids,
        approve: Box::new(|_, _| true),
    });

    let p_clone = provider.clone();
    let th = std::thread::spawn(move || {
        let _ = ssh::serve_stream(&mut server, &*p_clone);
    });

    // Request identities over stream
    let queried = request_identities(&mut client);
    assert_eq!(queried.len(), 2);
    let titles: Vec<&str> = queried.iter().map(|(_, t)| t.as_str()).collect();
    assert!(titles.contains(&"Work Key") && titles.contains(&"Personal Key"));

    // Sign request
    let payload = b"commit data to sign via ssh agent";
    let blob1 = k1.public_blob().unwrap();
    let sig1 = sign_request(&mut client, &blob1, payload).expect("sign succeeds");
    assert!(k1.verify(payload, &sig1).is_ok());
    assert!(k1.verify(b"different", &sig1).is_err());
    assert!(verify_blob(&blob1, payload, &sig1).is_ok());

    // Sign with unknown blob -> SSH_AGENT_FAILURE (5)
    let bad_blob = b"random-fake-public-key-blob";
    assert_eq!(
        sign_request(&mut client, bad_blob, payload),
        Err(SSH_AGENT_FAILURE)
    );

    // Drop client to terminate serve_stream thread cleanly
    drop(client);
    th.join().unwrap();

    // 5. Test denial in approval function
    let ids2 = session_identities(&s).unwrap();
    let (mut server2, mut client2) = UnixStream::pair().unwrap();
    let deny_provider = Arc::new(StaticKeys {
        ids: ids2,
        approve: Box::new(|title, _| title != "Work Key"),
    });
    let dp_clone = deny_provider.clone();
    let th2 = std::thread::spawn(move || {
        let _ = ssh::serve_stream(&mut server2, &*dp_clone);
    });

    // Work Key is denied
    assert_eq!(
        sign_request(&mut client2, &blob1, payload),
        Err(SSH_AGENT_FAILURE)
    );
    // Personal Key is approved
    let blob2 = k2.public_blob().unwrap();
    let sig2 = sign_request(&mut client2, &blob2, payload).expect("sign Personal Key");
    assert!(k2.verify(payload, &sig2).is_ok());

    drop(client2);
    th2.join().unwrap();

    // 6. Remove identity from vault and verify session_identities updates
    s.remove_item(&vault_id, &iid1).unwrap();
    let ids_after = session_identities(&s).unwrap();
    assert_eq!(ids_after.len(), 1);
    assert_eq!(ids_after[0].title, "Personal Key");

    s.remove_item(&vault_id, &iid2).unwrap();
    let ids_empty = session_identities(&s).unwrap();
    assert_eq!(ids_empty.len(), 0);
}
