# KuCoin custody prerequisite

This change adds an unused receipt-bound HTTP signing recipe and trusted setup
support for KuCoin's optional broker role. It does not change either production
KuCoin sender, construction rule, or Hosted admission. The maintainer must land
this prerequisite together with the stacked direct-cutover PR, never alone.

The cutover must install the API key, secret, passphrase, and any already
configured broker key only in the trusted provider. The venue retains opaque
refs and public broker metadata. One result carries Present(API key), request
HMAC, passphrase HMAC, and the optional broker HMAC under the same receipt.

The broker ref serves HMAC-SHA256 only, cannot be presented or interpolated as
credential bytes, and a secondary slot aliases the primary's one installed
ref. Rust Unicode White_Space trimming preserves today's native broker bytes;
an optional blank broker key is absent. A secondary cannot supply its own copy.

SDK, native, and WASM callers continue using the existing KuCoin constructors
until the stacked cutover. No Hosted bridge or live-request support is added.
Existing request, signing, and credential inventories remain unchanged here.
