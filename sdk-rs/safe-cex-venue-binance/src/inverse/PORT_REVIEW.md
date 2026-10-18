# COIN-M binding review

The behavior owner is the maintained `packages/safe-cex/src/exchanges/binance/`
`binanceinverse.{api,types,exchange,ws-public,ws-private}.ts` source. The native
binding is a module of the existing Binance family crate, with no new crate,
venue dependency edge or layer-policy change. Signing, HTTP, virtual clock,
Store, timers, socket lifecycle, public dispatch and depth algorithms retain
their existing shared owners.

`api.rs` calls the shared native NoProxy factory for precisely the source
`createPublicAPI` call at `binanceinverse.api.ts:23`. Private clients use the
existing routed factory. Public requests clear the API key and skip signing;
listen-key requests retain API-key authentication without HMAC. Other private
requests sign the exact repeated-query bytes. The bigint response transform
returns non-string native bodies unchanged, matching the maintained parser.

The following are measured receipts, not completeness claims:

- The actual constructor REST witness compiles against the preserved pre-port
  constructor, fails because the real clients are absent, and passes after
  restoration. The real socket constructors have the corresponding missing-slot
  compiled controls. REST observations include markets/brackets, inverse sizing,
  raw position info, unsigned demo listen keys, terminal order quantity and
  bigint order IDs, and malformed-envelope snapshot retention.
- `native_host_binanceinverse` constructs through the public production registry
  and performs an actual localhost account request. The server checks the API
  key and HMAC over the received query bytes, then the test reads native balance
  fields and disposes the actual exchange. Replacing the registry/host bodies
  with frozen master `3d557c5f8f` compiles and fails the constructor lookup;
  exact restoration passes. No venue operation is supplied by the fixture.
- The maintained-source REST oracle constructs the complete source class,
  BaseExchange, Store, API and sockets. Only the terminal Axios adapter is
  scripted. The native comparison uses the same response records and reads
  native contract bits before comparing the finite JSON result graph.
- A real encoded HTTP error envelope with an escaped lone UTF-16 surrogate
  compiled RED: the new Error message contained U+FFFD instead of U+D800.
  Using the installed native Error constructor restores the exact source code
  unit and retains name, code, response and the emission marker. The complete
  maintained source oracle independently asserts those Error fields.
- The actual OHLCV witness compiled RED on an absent global RegExp. The source
  uses a regex literal, so the binding now creates that literal with the existing
  shared regex primitive. Native/source witnesses pin the15m time window,
  unsigned public query, each native candle number and missing-volume NaN.
- The order witness replays25 maintained-source cases for split/attached orders,
  batch IDs/errors, cancellation grouping/partial progress, amendment ordering,
  leverage clamps and confirmed mode/margin transitions. Five compiled mutations
  (quantity, cancellation status, amendment price, leverage and mode) fail the
  actual native binding and pass after exact restoration. The missing groupBy
  import was itself a compiled production failure before the shared native
  grouping adapter was bound; no source order body was replaced by a fixture.
- The existing Binance generator owns the inverse constants and strict remote
  wire records. Its compiled live-source comparison checks all12 exported
  tables/constants and22 endpoint entries. Separate native record assertions
  read every field and signed-zero/overflow bits, including the response-array
  alias; qualified positional-array inputs are refused.
- The upstream registration comparisons execute the actual source and the native
  root services, registry descriptors/loaders, transport selection, metadata and
  numeric order policy. COIN-M is registered in each real owner. In particular,
  the compiled numeric witness failed after reaching the missing stock order
  metadata entry; the source-inherited infinity-stop/downsize flags repair it.
- `inverse_transport_binding` constructs the real parent with a recording
  private-routing provider and terminal HTTP host. Public market reads keep the
  dapi URL, omit the API key/signature and skip signing; the account-bracket read
  uses the provider's private URL and the actual signer. Replacing only the
  public NoProxy factory call with the existing routed factory compiles and
  fails the observed public request URL. Exact API bytes are then restored.

The typed Exchange facade retains the existing shared string/model admission
boundaries. Generated wire records retain the family's bounded numeric capture
and strict record profile. Finite fixtures do not establish arbitrary executable
getters, prototypes or sparse-array equivalence. Socket callback snapshot/alias
behavior is recorded in the socket manifest rows rather than claimed as full
native graph identity. Native sockets are reviewed separately by their owner;
all checkpoints are inherited with normal merges.

Reproduce from `sdk-rs` with `--jobs 1`: the `inverse_rest_binding`,
`inverse_socket_binding`, `types` targets in `safe-cex-venue-binance`, and
`native_host_binanceinverse` in `safe-cex-registry`. Live-source tests are
explicitly ignored in hermetic default runs and included by the existing
`venue-oracle:binance` all-targets gate. Supply the real installed compiler using
`SAFE_CEX_TYPESCRIPT_PATH`; do not replace source imports with fixture methods.
