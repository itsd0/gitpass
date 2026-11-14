# Pairing v2

The desktop shows one QR. The phone scans it and connects over the local network. The phone shows a 6-digit code. The person types that code on the desktop. The desktop then sends an encrypted grant. The Master Password is never an input to this protocol and is never placed in the QR, a frame, a log or an error.

This replaced the v1 offer / announce / grant frames. A sync-repo mailbox was considered after that design and was not built. The phone connects directly to the computer. `crates/gitpass-core` owns the key exchange and does not open sockets. Swift, Kotlin and JavaScript only move opaque bytes.

The threat notes are in [SECURITY_MODEL.md](../SECURITY_MODEL.md).

## QR

```text
gitpass-pair:2:qr:<sid>.<exp>.<port>.<addrs>.<host_pub>
```

`sid` and `host_pub` are unpadded URL-safe base64. `exp` is the absolute expiry in Unix seconds, 120 seconds after the desktop starts. `port` is the TCP port (1–65535). `addrs` is unpadded URL-safe base64 of a canonical JSON array of at most eight IP addresses. Names and ports are not accepted. Unspecified, multicast, IPv4 broadcast and non-canonical text are rejected. The whole QR is at most 4096 ASCII characters. A `gitpass-pair:1:` prefix is rejected.

The QR contains no Secret Key, no token and no Master Password. It does contain the session id, the expiry, the port, the addresses and the desktop's ephemeral public key.

## Frames

A frame is a big-endian `u32` length, one type byte, then the body. The length counts the type byte and is from 1 to 4096. A declared length of 0 is a bad message. A declared length above 4096 is rejected before the body is buffered. A leftover partial frame at the end of the session is truncated. The inbox cap is 16 KiB.

| Type | Byte | Body |
|---|---|---|
| Commit | 1 | session id (16) ‖ expiry `i64be` ‖ commitment (32) |
| Reveal | 2 | session id (16) ‖ `nonce_d` (32) ‖ desktop public key (32) |
| Open | 3 | phone public key (32) ‖ `nonce_p` (32) |
| Ready | 4 | empty |
| Grant | 5 | the sealed envelope |
| Fail | 6 | one byte: 1 refused, 2 expired |

The commitment is `SHA-256(phone_pub ‖ nonce_p)`. The phone sends it before revealing either value. The desktop then reveals `nonce_d` and repeats its public key, which must equal the key in the QR. The phone checks the hash and only then sends Open. A different key fails the hash.

## Transcript and keys

The transcript is, in order: version `u32be` (2), session id, expiry `i64be`, port `u16be`, the address JSON length `u16be` and those bytes, the desktop public key, the commitment, `nonce_d`, the phone public key, and `nonce_p`.

HKDF-SHA256 uses the X25519 shared secret as the input. The salt is `SHA-256` of the transcript. The info strings are `gitpass/pair/v2/aead` and `gitpass/pair/v2/sas`. The shared secret must be contributory. The all-zero output and a low-order peer key are rejected.

The code is the first four bytes of the SAS key, read as a big-endian integer, modulo 1_000_000, printed as six digits. The desktop compares the typed digits in constant time and does not return the expected code. Three wrong codes end the session. The phone does not learn that a code was wrong: a fail frame is either expired or a generic refusal.

The grant is the vault's XChaCha20-Poly1305 envelope (`seal_with_nonce` / `open`). The additional data is the bytes `gitpass/pair/v2` followed by the salt. There is no TLS on the socket.

## Payload

JSON object, exactly these keys:

`v`, `account_id`, `repo_owner`, `repo_name`, `branch`, `secret_key`, `sync_token`, `token_kind`.

`v` is 2. `token_kind` is `fine_grained_pat`. The token must be a fine-grained GitHub PAT (`github_pat_` plus ASCII letters, digits and underscore, at most 255 characters). A classic `ghp_` token is rejected. An extra `password` or `master_password` field is rejected.

The desktop takes the owner, repository, branch and token from the vault's git `origin` and the git-ignored `.gitpass-sync-token` file. A token typed for this session is used only when that file is absent, and it is not written to disk.

## Transport

The desktop binds one short-lived TCP port on its LAN addresses. It does not bind `0.0.0.0`. The production listener skips loopback. The phone connects to a numeric address from the QR. It does not resolve a name. The bytes are the frames above.

The Mac app is not sandboxed. No `com.apple.security.network.server` entitlement is set. macOS may still ask to allow incoming connections. iOS asks for local network access through `NSLocalNetworkUsageDescription`. Bonjour is not used, so `NSBonjourServices` is not set. On Android the socket is raw TCP, not HTTP, so the cleartext-HTTP network security config is unchanged.

A guest network that isolates clients can stop the connection. That is not something the protocol can repair.

## Replay and errors

A session id is single-use. The record is `.gitpass-pairing-consumed.json` (version 1, ids and expiries only) and is git-ignored. It is kept for 24 hours after the two-minute expiry. The desktop marks the id when the session starts. Success and three wrong codes mark it released. Cancel and lock drop the in-progress accept. The phone checks the record when it scans and marks the id when it finishes.

The peer clock may be up to 60 seconds ahead of `exp`.

Error text is one of: `pairing session expired`, `pairing session already used`, `pairing verification code does not match`, `pairing payload failed authentication`, `unsupported pairing protocol version`, `unrecognized pairing message`, `pairing key rejected`, `pairing payload field rejected`, `pairing state unavailable`, `no pairing in progress`, `too many wrong pairing codes`, `pairing message too large`, `pairing message truncated`, `pairing session does not match`, `pairing failed`. Transport failures shown by the apps are the fixed sentences `pairing could not connect`, `pairing timed out` and `pairing connection closed`. None of these include the token, the Secret Key, the Master Password, decrypted item data, or a filesystem path.

## What stays on the phone

After the grant, the person types the Master Password on the phone. The phone runs the existing first pull and stores the token in the platform keystore. Adding the phone to `devices.enc` happens later, when that phone unlocks with that password and the delivered Secret Key.
