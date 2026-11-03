# Coinbase bounded candle ranges

Coinbase and Coinbase Spot chart reads now honor the requested range using
sequential public requests of at most 300 candle buckets. The previous Rust path
limited the query's `limit` parameter but sent the entire start/end span, which
Coinbase rejects above 350 candles. Legacy TS instead clamped its derived range
to 300 candles and effectively ignored `from`; this change deliberately honors
larger ranges instead of silently dropping their earlier bars.

Inputs remain UTC milliseconds; Coinbase wire bounds and returned timestamps
remain integer Unix seconds. The existing eight REST granularity mappings and
USD/USDC product aliases are unchanged. Bounds are rounded as legacy TS did;
negative starts clamp to zero and future ends clamp to the command's clock.
Empty/reversed ranges finish with an empty array and no HTTP. Non-finite command
inputs remain refused by the engine's existing validation.

Each internal page is half-open. The wire end is its last included second, so
an inclusive venue end cannot introduce a 301st bucket. Consecutive pages leave
no gap. Only rows inside that page enter the accumulated timestamp map, and
duplicate timestamps collapse deterministically. Pages use the existing public
route and 334 ms request queue, and the command returns sorted bars only after
all pages succeed. A failed or malformed later page fails the read, never
publishes its successful prefix, and does not schedule another page. Command
range state is removed on success, transport/HTTP/parse refusal or queue refusal.
This is not managed-funnel adoption; the existing legacy HTTP and completion
inventory remains unchanged.

Production controls cover a real ARB-USDC catalog followed by 1,000 minute bars
in 300/300/300/100 windows, descending/duplicate/overlapping rows, explicit and
limit-derived ranges, epoch clipping, exact granularity conversion, and later
page failures plus late duplicates. The existing chronological candle fixture
now supplies bounds containing its unchanged 120/180-second bars. Actual
JS EngineHost/WASM/SDK coverage exercises the same 1,000-bar chart request.

No key, account, hosted-signing or write behavior changes; no new caller setup.
Coinbase's candle limit and granularity contract:
https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/products/get-product-candles
