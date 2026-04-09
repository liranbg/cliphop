#!/bin/bash
set -euo pipefail

# Creates a self-signed code signing certificate in the login keychain.
# Run this once per machine. The certificate persists across rebuilds,
# giving the app a stable identity for Keychain access and Accessibility.
#
# After running this, build-dmg.sh will automatically find and use
# the "Cliphop Signing" certificate.

CERT_NAME="Cliphop Signing"

# Check if certificate already exists and is valid for codesigning
if security find-identity -v -p codesigning | grep -q "$CERT_NAME"; then
    echo "Certificate '$CERT_NAME' already exists. Nothing to do."
    security find-identity -v -p codesigning | grep "$CERT_NAME"
    exit 0
fi

echo "==> Creating self-signed code signing certificate '$CERT_NAME'..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Generate key + self-signed certificate (valid for 10 years)
cat > "$TMPDIR/cert.cfg" <<EOF
[req]
distinguished_name = req_dn
x509_extensions = codesign_ext
prompt = no

[req_dn]
CN = $CERT_NAME

[codesign_ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
EOF

openssl req -x509 -newkey rsa:2048 \
    -keyout "$TMPDIR/key.pem" \
    -out "$TMPDIR/cert.pem" \
    -days 3650 -nodes \
    -config "$TMPDIR/cert.cfg" 2>/dev/null

# Bundle into PKCS12
openssl pkcs12 -export \
    -out "$TMPDIR/cert.p12" \
    -inkey "$TMPDIR/key.pem" \
    -in "$TMPDIR/cert.pem" \
    -passout "pass:cliphop-setup" 2>/dev/null

# Import identity (key + cert) via Swift — the macOS `security import` CLI
# tool often fails to import the private key from PKCS12 files.
cat > "$TMPDIR/import.swift" <<'SWIFT'
import Foundation
import Security

let p12Data = NSData(contentsOfFile: CommandLine.arguments[1])! as Data
let options: NSDictionary = [kSecImportExportPassphrase: CommandLine.arguments[2]]
var items: CFArray?
let status = SecPKCS12Import(p12Data as CFData, options, &items)
guard status == errSecSuccess,
      let arr = items as? [[String: Any]],
      let identity = arr.first?[kSecImportItemIdentity as String] else {
    fputs("ERROR: Failed to parse PKCS12 (status \(status))\n", stderr)
    exit(1)
}
let addStatus = SecItemAdd([
    kSecValueRef: identity,
    kSecAttrLabel: CommandLine.arguments[3],
] as NSDictionary, nil)
if addStatus != errSecSuccess && addStatus != errSecDuplicateItem {
    fputs("ERROR: SecItemAdd failed (status \(addStatus))\n", stderr)
    exit(1)
}
SWIFT

swift "$TMPDIR/import.swift" "$TMPDIR/cert.p12" "cliphop-setup" "$CERT_NAME"

# Trust the certificate for code signing — without this, codesign
# rejects the self-signed cert with CSSMERR_TP_NOT_TRUSTED.
security add-trusted-cert -p codeSign \
    -k ~/Library/Keychains/login.keychain-db \
    "$TMPDIR/cert.pem"

echo "==> Done. Certificate installed:"
security find-identity -v -p codesigning | grep "$CERT_NAME"
echo ""
echo "Now run: ./scripts/build-dmg.sh"
