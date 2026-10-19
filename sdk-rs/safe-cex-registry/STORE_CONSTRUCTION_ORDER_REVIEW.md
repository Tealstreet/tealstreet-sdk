# Canonical Store construction order

The Store graph is the value authority. Decoding it into StoreData and serializing
StoreUpdate before posting replaced source construction order with Rust field
order, recursively. Binance diagnostic fields moved after loaded/options/updateIds;
Apex zkLinkConfig moved from its appended position, and typed ticker output moved
volume/quoteVolume after fundingTime.

NativeStoreDelivery now sends the graph through the one StorePayloadPreparer and
captures its result with OwnedJsValue::from_js. Explicit snapshots call the real
facade serialize method. Only messageHandlerStats is projected for the existing
diagnostic processor. No model metadata, field sort, output normalization or
second payload algorithm is introduced. The typed worker library keeps its
existing processor frame and converts it once at the preparer input; that library
is preserved for its separately owned native-worker/session port.

The shared sanitizer follows ExchangeWorker.ts: a shallow root spread, copies of
updateIds/loaded/socketHealth, and object-rest removal of truthy order/position
info. Unchanged rows retain their source references until post capture. Market,
Ticker, Order, Position, DisplayedPosition, Balance, risk, active chaser/TWAP and
nested config records therefore keep their own construction order. Packed ticker
field order is a separate fixed protocol and is unchanged.

ApexOmniExchange.start previously created its zero ticker through typed Ticker.
It now creates the same canonical Js literal as the maintained constructor:
id, symbol, bid, ask, last, volume, quoteVolume, mark, index, percentage,
openInterest, fundingRate, fundingTime. It enters the real Store facade directly;
subsequent patches replace values without moving existing keys.

## Executable evidence

- native_store::construction_order_tests invokes the real Binance constructor,
  DefaultStore, subscription, throttle and delivery. Six complete maintained-source
  profiles rotate each nested object's insertion order and retain dynamically
  appended root config. OwnedJsValue equality checks key sequences and values
  before JSON. The baseline test compiled and failed at MODEL-CONSTRUCTION-ORDER.
- actual_okx_delivery_retains_own_undefined_stark_config checks the real OKX
  constructor, live config-clear notification and final snapshots. Maintained
  DefaultStore/worker profiles retain starkConfig as an own undefined property;
  no DTO or JSON projection may turn it into absence. A compiled omission fault
  fails the named STORE-OWN-UNDEFINED snapshot assertion; exact restoration
  replays both actual native-delivery tests.
- native_account_store checks the real account startup full snapshot's key list
  before JSON; its existing HTTP/gateway and trailing-throttle assertions remain.
- Apex binding::start_ticker_retains_maintained_constructor_key_order executes
  startup and compares own keys with the maintained whole-class lifecycle oracle.
  It compiled and failed on the original volume/quoteVolume placement.
- Existing worker tests retain 243 version decisions, 358 ticker transitions,
  48 sanitization profiles and 1224 preparation transitions. The direct native
  graph observer additionally checks the source's three copies and retained
  balance/untouched-order aliases; nonfinite values and signed zero remain native.

Standalone typed REST-return APIs are a separate observable own-key-order FIX.
They discard construction order before output capture, so this Store projection
cannot reconstruct it. A follow-up census must enumerate methods and venues and
compare the actual mapper construction orders before choosing their repair.
