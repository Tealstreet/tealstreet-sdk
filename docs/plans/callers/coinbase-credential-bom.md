# Coinbase pasted credential BOM normalization

Coinbase and Coinbase Spot accept U+FEFF alongside existing whitespace at the
edges of API key names, secrets and downloaded credential JSON. Base64 secrets
also discard U+FEFF inside the encoded text, matching legacy JavaScript's
whitespace normalization. JSON field precedence, key types, size limits and
private startup routing stay unchanged; key-name interiors are not rewritten.

SDK and NX callers need no changes or credential migration. Pasted credentials
produce the same startup JWT as their normalized input. This adds no hosted
signing admission or deletion prerequisite.
