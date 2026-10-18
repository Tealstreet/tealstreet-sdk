# Private native owner observations

`Auth` and `PrivateState` expose raw inputs and have no Deserialize implementation.
They are observed APIs, never typed or non-decoder classifications. The actual
compiler signatures are retained individually in their contract rows.

Both contracts declare `requiredFeatures: ["native"]`. The canonical driver keeps
an explicit default `native-gated`, `observed: false` receipt and runs fresh native
probes. `scripts/private-native-owner-contracts.mjs` verifies both receipts per
owner, actual compiler features, actual execution and zero failed observations.
`--native` selects the executed native receipts directly for compiled controls.
The pre-contract `--expect-unclassified` run recorded two named native failures
after full discovery of 197 Deserialize owners; compile failures are not controls.

The observation support constructs production `native_dependencies` and the real
`ExchangeOwner`. It borrows the existing private state through its original Rc.
There is no replacement PrivateParent, PrivateBase, Auth or event mapper. Only the
clock/task/socket terminal fixture is scripted; unexpected HTTP always panics.

| Actual API | Two finite profiles and native observations |
|---|---|
| Auth::hmac_sha256_hex | Distinct ASCII secret/payload pairs; independently specified HMAC-SHA256 digests and native string unit counts. The constructed Spot authentication witness separately exercises actual parent options/clock/socket send. |
| PrivateState::handle_events | Full Futures NEW-order wrappers with distinct IDs/amounts; actual Store ID plus amount/price binary64 bits. |
| ConnectBindings::emit | Distinct messages; actual Base error listener text and argument count. |
| ConnectBindings::log | Distinct messages/severities; actual Base log listener text, severity and argument count. |
| ConnectBindings::create | Distinct URLs; actual core socket creation, terminal dial URL, native returned record keys and retained state Rc identity. |
| ConnectBindings::add_listener | Actual core socket/message registration, terminal full frames and actual Store ID/amount/price bits. |

`python3 safe-cex-venue-binance/scripts/private-native-owner-controls.py`, from
`sdk-rs`, faults each production method independently. Each fault must compile
and yield its named owner output-mismatch receipt; exact source restoration must
then pass. The script restores bytes in `finally` and records the source SHA.
A local `target/private-native-owner-controls/STOP` marker stops only between
controls, after restoration, for the serialized array migration handoff.

These twelve observations establish the stated finite native profiles. They do
not establish arbitrary-JS input admission, custom accessors/reentry, arbitrary
UTF16 messages, or Wasm execution, and do not erase the maintained family UTF8 or
typed-output boundaries. The separate PrivateSocket constructor/Deref record
remains a typed capability classification with separate actual native tests.
