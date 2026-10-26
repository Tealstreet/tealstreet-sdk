# Lighter beta readiness

Status: local public-data slice on the managed funnel in #2230. Grouped limit
TP/SL is owned by kuhwxi as a separate slice; this slice does not enable it.

The beta runs through `hosts/wasm/js/host.mjs`. A live public recording on #2230
captured four HTTP replies and 15,401 socket frames, but replay found dropped
stats rows and book aggregation differences. Native tests alone are not beta
qualification. The original tape and failing report remain in scratch.

## Public data corrections

- Decode base and USD open interest separately. Prefer base open interest when
  both exist, regardless of JSON order, so the rest of the stats row survives.
- Seed last price from the accepted market snapshot before socket stats arrive.
- Aggregate REST depth at the market's native amount precision, without binary
  addition residue. Deltas replace a complete parsed native size, without
  addition. Preserve price ordering, full depth and zero handling.
- On public close, request one prompt managed REST snapshot for each subscribed
  book. Keep the existing single-flight and pacing rules. An older REST reply
  cannot overwrite a newer socket snapshot. Repeated closes cannot multiply
  in-flight requests; unsubscribed books remain quiet.

The first four added public tests run on unmodified production source:
13 native controls pass and four intended assertions fail. Additive source
controls will cover key order, optional values, reconnect flapping and a fresh
socket snapshot beating the recovery read. Do not relax replay tolerances or
filter the captured tape.

## Remaining beta blocker

Grouped limit entry with TP/SL must use one type-28 OTO/OTOCO transaction and one
nonce through TxRecipe. Confirm the parent and every child before success; an
unconfirmed child means Unknown with proven IDs and uncertainty retained, with
no replay. Lock every child ID. Definitive pre-execution 21104 may reseed once;
408/5xx cannot. Invalid triggers or unsupported splits refuse before signing.
Kuhwxi owns its financial tests and review before credentialed smoke.

Reviewer: dazzy3. No engine, crypto, registry, budget or inventory edit is planned
for the public-data slice. The final JS/WASM build must replay the retained live
tape and pass a fresh live-public run before claiming beta readiness.
