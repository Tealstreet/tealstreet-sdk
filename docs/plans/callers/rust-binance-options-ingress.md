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
