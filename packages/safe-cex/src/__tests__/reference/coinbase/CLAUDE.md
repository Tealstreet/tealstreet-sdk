# Coinbase replay reference

Frozen TypeScript Advanced Trade adapter retained only for Rust replay comparison
and legacy audit tests. Production accounts use the Rust venue. Keep imports
relative to this test-only directory; never route a production registry here.
Declared venue corrections belong in the harness known-TS-fixes entry, with
independent documented-payload oracles and negative controls.

The native-store cutover removed BaseExchange's TS store writers. Retained
REST snapshot tests explicitly emit SCXF from the reference adapter after a
successful read; the production BaseExchange remains frame-consumer only.
