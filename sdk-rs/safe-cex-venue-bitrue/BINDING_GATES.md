# Bitrue native class binding

The concrete Exchange implementation assembles the existing source body ports on
shared js_runtime BaseExchange, HTTP clients, Store and BaseWebSocket cores.
`tests/exchange_binding.rs` constructs it and invokes all sixteen required trait
methods against scripted transports. getAccount, fetchTickers,
changePositionMode, scheduleCancel and fetchPublicTrades inherit the exact
`Not implemented` rejection from base.ts lines734,759,832,989,1138;
changeMarginType retains Bitrue's own refusal. These cases have independent named
assertions rather than successful empty results.

Direct HTTP signing is also exercised through the production registry/native
host against a local HTTP server. That server independently calculates the
actual timestamp/method/path HMAC and checks the API-key/signature headers.
Both single and batch client-ID validation reject before any HTTP request. A
successful two-order batch checks each caller ID on the wire. Private key
failure exhausts the four source attempts without dialing/subscribing a private
socket. A native Buffer ping frame distinguishes ordinary public Number JSON
parsing from private big-integer string rewriting. Disposal closes both sockets.

The binding retains existing declared typed helper/JSON domain choices; it does
not widen them to arbitrary source objects. The former HOW transport refusal is removed by shared routing PR185
(overseer queue ddi2il-12). Source creation selects provider routing, forces20s,
authorizes after venue signing, and installs its BigInt adapter only under the
original tealstreetTunnel condition. The routed E2E covers both flag states.
Shared worker RPC comes from #173, with no
venue-local duplicate. See the manifest class/constructor notes for these
adapter choices and the existing helper-level divergences.

Validation on the branch:

- Full Bitrue crate: 65 tests across26 targets,0failed/ignored, including actual
  TypeScript oracles (`SAFE_CEX_TYPESCRIPT_PATH` set; `--include-ignored`).
- `scripts/binding-controls.py`:16 independent dispatch mutations compile and
  exit101 at their named BIND assertion; each restored production file passes.
- Native registry Bitrue HMAC test and12 existing registry-construction tests.
- Strict all-target host and wasm library clippy; scoped registry clippy.
- Global parity:check and extractor check; layer and registry checks.

The combined shared dependency gates pass58 JS-runtime tests,7 cancellation
inventory tests and the live TypeScript override inventory. The override audit
combines #175 and Crypto.com's source-matched disposal #186/#158. Four Bitrue
body references are recorded explicitly: dispose/start/margin refusal in the
member dispatcher and the existing maximum-leverage helper delegation.

The routing follow-up adds one E2E and two compiling disconnect controls in
`scripts/routing-controls.py`. Earlier sixteen method controls remain unchanged.
