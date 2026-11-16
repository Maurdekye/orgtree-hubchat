Throwaway certificates for `tests/tls_trust.rs` and the device-trust check in CI. They protect nothing.

| File | What it is |
|---|---|
| `ca.pem` | a test certificate authority (CA). Its private key was deleted when these files were made, so no further certificate can be issued from it. |
| `hub.pem`, `hub-key.pem` | a server certificate from that CA for the IP address `127.0.0.1` and the name `localhost`, and its key |
| `wrong-name.pem`, `wrong-name-key.pem` | a server certificate from the same CA for the name `other.test` only |
| `other-ca.pem` | a second CA, which the tests never trust (key deleted too) |
| `other-hub.pem`, `other-hub-key.pem` | a server certificate from the second CA for `127.0.0.1` and `localhost` |
| `self-signed.pem`, `self-signed-key.pem` | ONE self-signed certificate for `127.0.0.1` and `localhost` that is also its own CA, and its key: what a single `openssl req -x509` makes. The apps refuse it as a hub's certificate even when the device trusts it. |

All are EC P-256 and valid until 2126. The server keys are public by being here. Never install `ca.pem` or
`self-signed.pem` on a real device: only on a machine that is thrown away afterwards (a CI runner, an emulator).

Made with OpenSSL 3.5 (`openssl req -x509` for each CA with `basicConstraints=critical,CA:TRUE`;
`openssl x509 -req` for each server certificate with `subjectAltName`, `extendedKeyUsage=serverAuth` and
`basicConstraints=critical,CA:FALSE`; `self-signed.pem` with one `openssl req -x509` and `subjectAltName`,
which OpenSSL's stock settings mark `CA:TRUE`).
