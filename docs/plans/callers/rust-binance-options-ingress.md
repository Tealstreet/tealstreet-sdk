# Binance Options compiled command ingress

The Binance adopter enables the engine's command-options feature alongside
product-semantics, matching its advertised Options capability. This links
fetchOptionChain, fetchOptionExpiries and fetchAccountGreeks into isolated
native and browser/wasm command ingress, using the existing handlers and
caller custody. Other Binance variants keep their existing capability records.

The production regression decodes all three JSON commands and drives the real
Binance binding, checking the expected read or cached-expiry completion. The
existing compiled web family conformance control is RED on the release tree
at fetchOptionChain and must pass on the corrected isolated family. There is
no financial mapping, endpoint, signing or transport-policy change.

The corrected isolated Node/Web family measures 1,460,760 bytes locally. ddi
approved the 1429 KiB (1,463,296-byte) cap, measured ceil KiB plus 2 KiB.
Options is outside Sam's cutover scope; splitting it into a separate family to
recover the existing Binance variants' linked size is the first follow-up after
cutover. CI remains the authoritative family measurement record.
