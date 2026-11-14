# Handover test vectors

All inputs here are synthetic or published RFC test data. They are unsuitable
for real accounts.

`rfc9180-x25519-chacha20.json` contains the first three base-mode encryption
answers for the suite in [RFC 9180 appendix A.2.1](https://www.rfc-editor.org/rfc/rfc9180.html#appendix-A.2.1).
The input was obtained from the CFRG reference implementation's
`test-vectors.json`. The server tests check both consecutive messages and the
single-message opener.

`delegated-handover-v1.json` uses the same suite with the hosted handover's
application context. Its fields use standard padded base64. It was generated
independently using Python cryptography 46.0.5, SHA-256/HMAC and the RFC's labeled
extract/expand operations; the Rust test reproduces its encapsulation and
ciphertext and opens the fixed answer. The recipient's raw private input is
32 repeated `0x24` bytes; sender `DeriveKeyPair` input is 32 repeated `0x42`
bytes. These deterministic inputs exist solely for interoperability tests.

Consumers should compare the decrypted JSON fields, use the literal `info` and
`aad` strings, and reject any change to the authenticated context. The fixture's
deadline is relative to its synthetic test clock, not a live server deadline.
