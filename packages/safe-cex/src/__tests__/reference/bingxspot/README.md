# BingX Spot fixture reference

`BingxSpotReference` is an independent TypeScript oracle with injected fixture
transport. It maps the SDK's requested Spot surfaces without using the Rust
codecs. It is deliberately absent from production and legacy live adapter
construction. The SDK continues to refuse `bingxspot` through `assertBetaVenue`.

The shared documented fixtures live at
`sdk-rs/venues/bingx/tests/fixtures/spot`, with provenance and adaptations in
that directory's README. Native tests cover authenticated/public WS lifecycle,
execution dedupe, write ambiguity and startup races; this oracle independently
checks REST mappings, base/quote sizing, exact identifiers and signing bytes.

See `sdk-rs/venues/bingx/SPOT.md` for official protocol links, permissions,
transport requirements, incomplete-history policy and the NX/smoke follow-ups.
