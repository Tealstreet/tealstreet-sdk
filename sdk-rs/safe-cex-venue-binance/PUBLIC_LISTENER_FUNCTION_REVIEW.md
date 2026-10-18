# Public listener function roles

These nine functions accept typed host/registry, callback, String and counter
capabilities. None is a raw-message decoder. Each exact compiler API has its own
non-decoder classification; these diagnostic receipts do not claim to execute
the capability. The actual Book decoder and six callback/future alias reviews
remain separate and unchanged.

| Function | Reviewed role | Real native execution witness |
| --- | --- | --- |
| payload | Build an outbound method/params/id record from typed arguments | Exact subscription/unsubscription field order, method/topic and wire ID values |
| add_handler | Append a captured Parser Rc to the current typed handler row | Two actual subscriptions preserve first parser identity and append the second |
| remove_handler | Remove the exact Parser Rc and optionally delete an empty row | First cleanup retains the sibling by identity; last legacy depth cleanup keeps an empty row |
| topic_add | Increment the typed subscription count with source falsy-zero fallback | Two native subscriptions produce count 2 before any frame/HTTP settlement |
| legacy_topic_remove | Decrement/delete the legacy numeric topic counter | Cleanup transitions 2 to 1, then removes the last topic |
| depth | Construct the retained book/parser/sequence state and wait/cleanup continuations | Real delayed connection, held HTTP, two independent books, updates and cleanup |
| listen_futures_book | Bind the Futures venue to the depth lifecycle | Actual Futures owner reaches the FAPI route with the flat snapshot query |
| listen_pm_book | Bind the PM venue to the depth lifecycle | Actual PM owner reaches the public FAPI route with the flat snapshot query |
| listen_spot_book | Bind the Spot venue to the depth lifecycle | Actual Spot owner reaches the spot API route with the flat snapshot query |

The tests construct native_dependencies and the actual ExchangeOwner/PublicSocket
for Futures, PM and Spot, holding only terminal HTTP and websocket events. No
replacement ListenerHost or helper under test supplies the result. Observations
read parser and book Rc identities, callback order, native topic/book number bits,
and handler/topic state before JSON projection. Payload fields and ID values
are separately checked on the actual socket wire, after production encoding. Both subscriptions retain their
own book and only the exact surviving parser can observe a later real frame.
The existing native overflow cases and maintained 18-case-per-family listener
oracle remain unchanged; they cover source numeric guards and error/lifecycle
behavior beyond these normal two-subscription observations.

Nine independent production body faults must compile and fail their named real
native test, followed by byte-exact source restoration and a passing rerun:
wrong outbound method key, prepend instead of append, inverted identity filter,
wrong increment/decrement, wrong symbol capture and each wrong family selector.
A compilation error or zero-test result never counts as a semantic control.

These are individual typed role classifications, not blanket decoder exemptions,
claims of arbitrary JavaScript admission, or closure of other Binance owners.
ASCII symbols, ordinary data dictionaries and normal numeric subscription counts
are the constructed profile here. Getter/prototype mutation, arbitrary counters,
future cancellation and unrelated feature/wasm combinations are not established
by this cohort. No production function body is permanently changed.
