#!/bin/sh
set -eu
directory=$1
openssl=$2
mkdir -p "$directory"
"$openssl" req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
    -keyout "$directory/ca.key" -out "$directory/ca.pem" -days 2 -sha256 \
    -subj /CN=Bhasa-Test-CA -addext basicConstraints=critical,CA:TRUE \
    -addext keyUsage=critical,keyCertSign,cRLSign
"$openssl" req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
    -keyout "$directory/other.key" -out "$directory/other.pem" -days 2 -sha256 \
    -subj /CN=Bhasa-Untrusted-CA -addext basicConstraints=critical,CA:TRUE \
    -addext keyUsage=critical,keyCertSign,cRLSign
"$openssl" req -new -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
    -keyout "$directory/server.key" -out "$directory/server.csr" -subj /CN=localhost
cat > "$directory/server.ext" <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1
EOF
"$openssl" x509 -req -in "$directory/server.csr" -CA "$directory/ca.pem" \
    -CAkey "$directory/ca.key" -set_serial 2 -out "$directory/server.pem" \
    -days 2 -sha256 -extfile "$directory/server.ext"
"$openssl" pkcs8 -topk8 -in "$directory/server.key" -out "$directory/encrypted.key" \
    -passout pass:test-only-password
cat > "$directory/cn.ext" <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature
extendedKeyUsage=serverAuth
EOF
"$openssl" x509 -req -in "$directory/server.csr" -CA "$directory/ca.pem" \
    -CAkey "$directory/ca.key" -set_serial 3 -out "$directory/cn.pem" \
    -days 2 -sha256 -extfile "$directory/cn.ext"
