Throwaway certificates for `tests/tls_trust.rs` and the device-trust check in CI. They protect nothing.

| File | What it is |
|---|---|
| `ca.pem` | a test certificate authority (CA). Its private key was deleted when these files were made, so no further certificate can be issued from it. |
| `hub.pem`, `hub-key.pem` | a server certificate from that CA for the IP address `127.0.0.1` and the name `localhost`, and its key |
| `wrong-name.pem`, `wrong-name-key.pem` | a server certificate from the same CA for the name `other.test` only |
| `other-ca.pem` | a second CA, which the tests never trust (key deleted too) |
| `other-hub.pem`, `other-hub-key.pem` | a server certificate from the second CA for `127.0.0.1` and `localhost` |

All are EC P-256 and valid until 2126. The server keys are public by being here. Never install `ca.pem` on a
real device: only on a machine that is thrown away afterwards (a CI runner, an emulator).

Made with OpenSSL 3.5 (`openssl req -x509` for each CA with `basicConstraints=critical,CA:TRUE`;
`openssl x509 -req` for each server certificate with `subjectAltName`, `extendedKeyUsage=serverAuth` and
`basicConstraints=critical,CA:FALSE`).
