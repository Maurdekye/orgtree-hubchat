`test-cert.pem` and `test-key.pem` are a self-signed certificate for `localhost` and its key, made for
`tests/server.rs` only (`openssl req -x509 -newkey ec ...`, valid until 2126). The key is public by being here:
it protects nothing and must never be used for a real server.
