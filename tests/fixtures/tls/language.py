import os
from pathlib import Path
import select
import socket
import ssl
import subprocess
import sys
import threading

server_exe, client_exe, cert_dir = sys.argv[1:]
certs = Path(cert_dir)
context = ssl.create_default_context(cafile=str(certs / 'ca.pem'))

def server(requests):
    child = subprocess.Popen([server_exe, str(certs / 'server.pem'), str(certs / 'server.key'), '0', str(requests)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if not select.select([child.stdout], [], [], 5)[0]:
        child.kill(); child.wait(); raise AssertionError('HTTPS listener startup timeout')
    port = int(child.stdout.readline())
    return child, port

def finish(child):
    try:
        out, err = child.communicate(timeout=15)
        assert child.returncode == 0, (child.returncode, out, err)
        assert not out and not err, (out, err)
    except BaseException:
        child.kill(); child.wait(); raise

child, port = server(3)
try:
    # A rejected peer must not terminate the Bhasa server.
    wrong = ssl.create_default_context(cafile=str(certs / 'other.pem'))
    try:
        with socket.create_connection(('127.0.0.1', port), timeout=3) as raw:
            wrong.wrap_socket(raw, server_hostname='localhost')
        raise AssertionError('untrusted server certificate accepted')
    except ssl.SSLCertVerificationError:
        pass
    # Fragmented UTF-8 request and verified HTTPS response.
    with socket.create_connection(('127.0.0.1', port), timeout=3) as raw:
        with context.wrap_socket(raw, server_hostname='localhost') as stream:
            stream.sendall(b'GET /health HTTP/1.1\r\nHost: localhost\r\n')
            stream.sendall(b'\r\n')
            response = bytearray()
            while True:
                chunk = stream.recv(4096)
                if not chunk: break
                response.extend(chunk)
            head, body = bytes(response).split(b'\r\n\r\n', 1)
            assert head.startswith(b'HTTP/1.1 200 OK\r\n'), response
            assert b'Content-Length: 11' in head and body == b'{"ok":true}', response
    # Compiled Bhasa client uses generic owned TLS payloads and name verification.
    result = subprocess.run([client_exe, str(port), 'localhost', str(certs / 'ca.pem')], capture_output=True, timeout=10)
    assert result.returncode == 0 and result.stdout == b'0\n{"ok":true}\n' and not result.stderr, result
    finish(child)
except BaseException:
    child.kill(); child.wait(); raise

for name, ca in [('wrong.example', 'ca.pem'), ('localhost', 'other.pem')]:
    child, port = server(1)
    try:
        result = subprocess.run([client_exe, str(port), name, str(certs / ca)], capture_output=True, timeout=10)
        assert result.returncode == 0 and result.stdout == b'10001\n' and not result.stderr, result
        finish(child)
    except BaseException:
        child.kill(); child.wait(); raise
print('compiled TLS and HTTPS checks passed')
