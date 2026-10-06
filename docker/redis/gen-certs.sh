#!/usr/bin/env bash
# A throwaway CA, a server certificate for 127.0.0.1 and one for the wrong name (tests/redis_tls.rs), into ./tls
# (git-ignored). Valid 30 days. Never reuse these outside tests.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p tls
cd tls
conf=$(mktemp)
trap 'rm -f "$conf"' EXIT
cat > "$conf" <<'CONF'
[ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
[srv]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = IP:127.0.0.1
[wrong]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = DNS:wrong.example
[req]
distinguished_name = dn
[dn]
CONF
openssl req -x509 -newkey rsa:2048 -nodes -keyout ca.key -out ca.crt -days 30 -subj "/CN=tableiq-test-ca" -config "$conf" -extensions ca 2>/dev/null
openssl req -newkey rsa:2048 -nodes -keyout server.key -out server.csr -subj "/CN=127.0.0.1" -config "$conf" 2>/dev/null
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out server.crt -days 30 -extfile "$conf" -extensions srv 2>/dev/null
# The same CA's certificate for a name that is not the server's: a connection must refuse it.
openssl req -newkey rsa:2048 -nodes -keyout wrong.key -out wrong.csr -subj "/CN=wrong.example" -config "$conf" 2>/dev/null
openssl x509 -req -in wrong.csr -CA ca.crt -CAkey ca.key -CAcreateserial -out wrong.crt -days 30 -extfile "$conf" -extensions wrong 2>/dev/null
rm -f server.csr wrong.csr ca.srl
# The container's redis user reads the (throwaway) key.
chmod 644 server.key wrong.key ca.key
echo "wrote $(pwd)/{ca,server,wrong}.crt — SSL_CERT_FILE=$(pwd)/ca.crt trusts them"
