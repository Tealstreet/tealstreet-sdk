# Bitrue concrete receiver review

The ordinary compiled inventory originally stops at 32 methods on
`Scope<binding::Extra>`, before it can count the crate's aliases. The first
reported `ParseHost::lookup` is not the whole refusal set. This change registers
exact methods against the compiler's complete impl-item spans; it adds no Scope
exemption or identity-conversion fallback and changes no production algorithm.

| Impl | Individually reviewed methods | Native witness |
| --- | --- | --- |
| `ParseHost` | `lookup` | Installed SELL, absent key, inherited original handle; existing private frame commits SELL to the production Store |
| `SubscriptionHost` | `channels_for_each` | Ordered channels, undefined return, early rejection with original error identity |
| `PublicBodyHost` | `text`, `global`, `object` | Supplementary text, original object/global handles, actual public socket's bound WebSocket capability |
| `StreamHost` | `global` | Original installed enum-table identity |
| `ReadHost` | `text`, `get`, `set`, `split_at`, `object`, `table`, `console` | UTF-16 text, own undefined, negative-zero bits, original referents, selected/missing segment, actual enum and runtime sink |
| `TradingHost` | `has`, `remove_key` | Own/inherited undefined versus absence, primitive refusal, actual deletion and nonconfigurable refusal |
| `LifecycleHost` | `builtin` | Original installed timer capability |
| `PrivateHost` | `contains`, `starts_with`, `split`, `transport`, `create_hmac`, `define`, `member`, `class_environment`, `named_function` | String operations; valid/invalid adapter and venue admission; known HMAC; own referent; actual source success-code body; installed retry-delay bits; callback argument bits/name/return identity |
| `CompositionHost` | `define`, `rule`, `grafana`, `binding`, `construct_socket`, `member`, `named_function` | Own referent; real regexp match/refusal; exact endpoint order; actual factory identity; both actual socket classes and parent identity; maintained timestamp body; callback name/identity/result bits |

`receiver_tests.rs` obtains its Scope only from `BitrueExchange::new`, including
the normal BaseExchange, DefaultStore, global installers, API clients and both
socket constructors. Only terminal runtime/IO are supplied. Unexpected HTTP
panics. Tests do not manufacture a replacement Scope, mapper, parent or crypto
implementation. Class-environment provisioning is explicitly a metadata no-op;
the real installer owns delays and the real constructor owns disposal.

The receiver records distinguish native capability forwarding from delegation
to shared string/regexp/crypto/transport implementations. They do not replace
those shared owners' decoder obligations. The finite observations do not claim
whole-venue parity, arbitrary JavaScript input coverage or alias completion.

Run serially from `sdk-rs`:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-bitrue --lib receiver_tests
python3 safe-cex-venue-bitrue/scripts/receiver-controls.py
RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR="$PWD/target/accepted-inventory" node safe-cex-parity/scripts/accepted-set.mjs --package=safe-cex-venue-bitrue --inventory --jobs=1
```

The control script independently faults one production operation in every one
of the nine reviewed impls. Each fault must compile and fail its named native
witness; the exact original source bytes must then pass. Logs and source hashes
are written to `target/bitrue-receiver-controls`. Never run another compiler or
inspect changing production bodies while controls are active. A `STOP` file in
that directory requests a stop after the current source has been restored.

The nine independent controls all passed. Canonical replay clears the receiver
guard and stops next at `public_bodies::NativeCallback` (missing explicit alias
classification). Compiler discovery contains seven public aliases: that callback,
`collection_reads::Callback`, `trading_rest::{Body,AsyncCallback}`,
`position_metadata::PositionMetadataHandle`, and
`wire::{FetchContractsResponse,FetchKlineResponse}`. They remain a separate
follow-up cohort; this is not a claim that the full accepted inventory is green.
