# Stable local code signing on macOS

Ad-hoc signed builds get a new identity on every rebuild, so the Keychain
"Always Allow" for `app.gitpass.desktop.biometric` doesn't carry over. If you
sign every local build with the same self-signed certificate, the designated
requirement (`identifier "dev.gitpass.desktop" and certificate leaf = H"<sha1>"`)
stays the same, so the Keychain keeps trusting the app across rebuilds.

## One-time setup

```sh
T=$(mktemp -d); cd "$T"
cat > c.cnf <<'EOF'
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=gitpass Local Signing
[ext]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
EOF
openssl req -x509 -newkey rsa:2048 -nodes -keyout k.pem -out c.pem -days 3650 -config c.cnf
P=$(openssl rand -hex 16)
openssl pkcs12 -export -legacy -inkey k.pem -in c.pem -out c.p12 -passout pass:$P
security import c.p12 -k ~/Library/Keychains/login.keychain-db -P "$P" -T /usr/bin/codesign
rm -P k.pem c.p12; cd /; rm -rf "$T"     # never commit or keep the key/p12
security find-identity -p codesigning     # "gitpass Local Signing" (CSSMERR_TP_NOT_TRUSTED is OK)
```

Optional (avoids codesign keychain prompts; asks for your login password):
`security set-key-partition-list -S apple-tool:,apple:,codesign: -s ~/Library/Keychains/login.keychain-db`

## Building

The identity comes from an environment variable, not `tauri.conf.json`, so
CI and other machines without the certificate still build normally:

```sh
cd apps/desktop
APPLE_SIGNING_IDENTITY="gitpass Local Signing" npx tauri build --bundles app
codesign -dv --verbose=4 src-tauri/target/release/bundle/macos/gitpass.app   # Authority=gitpass Local Signing
```

After the first launch of a build signed this way, click **Always Allow** once
at the Keychain prompt. Later builds signed with the same certificate shouldn't prompt again.
