# Concrete Polymarket binding evidence

Constructor capture reads the original imported `ethers.Wallet` and
`SDK.OrderBuilder` values before invoking their construct capability. The shared
dotted-call convenience wrapper retains method receivers but is not the imported
constructor. The 15 wallet-backed `exchange_binding` cases from the #848 failure
cohort fail on master `a2efcc3246` and pass with this raw-member capture; their
address, signing, wire-payload and private Store assertions remain unchanged.

Implementation foundation: `0c95c79ff1`. Native waiter repair: `9041a2f57a`.
Supplied-account regressions: `7ce9221719` and fixture correction `2d171832bf`.
Latest master/Balance integration measured at `a799d01d06`.
Final facade/socket/worker integration: `8a015b33ed` (including #171 facade fix
`3a63e1bd28` and independently reviewed #152 `231609b464`).
Shared dependencies: socket registration #152, data descriptors #169, account carrier #171.

The full configured venue suite passed **198 tests across 29 targets** after the
account carrier integration. Its single native signing test executes **96** cases
against the maintained TypeScript signing helper and installed official V2 SDK:
six tick sizes, four signature types, both sides, and both contract families.
It compares signatures, native order fields, own-key order, and exact wire bytes.
The final facade integration also passes three SDK construction/patch tests,
54 shared-runtime regressions, and six enabled cancellation inventory tests
(one pre-existing export test is intentionally ignored).

After merging the source currency Balance model from master, all **15 concrete
E2Es**, **13 registry tests**, and **two real native HTTP tests** passed. The latter
construct through the production registry and reqwest adapter, changing only the
origin to a local HTTP server. Supplied credentials, funder holdings addresses,
signer types 0–3, and supplied-secret HMAC over exact transmitted order bytes are
checked through the concrete constructor. No production code invokes Node.

Scoped parity passed with **141 rows / 439 public Rust items**, no exemptions.
Global parity passed with **7,459 rows / 17,082 public Rust items**, zero findings.
Host all-target clippy, wasm library clippy, registry-test clippy, and formatting
passed. Final integration also passes public SDK host all-target and wasm library
clippy, covering the facade initializer/patch forwarding required by #171.

## Controls

`tests/binding_controls.py` measured all **16** entrypoint mutations: each compiled,
failed its named concrete E2E, and passed after restoration. The executable method
bodies exercised by these controls remain present after shared dependency merges.

| Entry point | Named concrete E2E |
| --- | --- |
| `place_order` | `concrete_place_order_posts_real_signed_gtc_payload` |
| `start` | `concrete_markets_and_public_start` |
| `get_account` | `concrete_wallet_account_matches_ethers_address` |
| `validate_account` | `concrete_validate_derives_real_l1_signature_and_l2_headers` |
| `fetch_positions` | `concrete_fetch_positions_uses_holdings_and_keeps_snapshot_pure` |
| `fetch_orders` | `concrete_fetch_orders_maps_all_pages_without_store_commit` |
| `fetch_markets` | `concrete_markets_and_public_start` |
| `fetch_tickers` | `concrete_markets_and_public_start` |
| `fetch_symbol_max_leverage` | `concrete_constructor_account_and_source_controls` |
| `set_leverage` | `concrete_constructor_account_and_source_controls` |
| `change_position_mode` | `concrete_constructor_account_and_source_controls` |
| `change_margin_type` | `concrete_constructor_account_and_source_controls` |
| `update_order` | `concrete_update_order_cancels_then_signs_replacement` |
| `cancel_orders` | `concrete_cancel_orders_batches_only_ids` |
| `schedule_cancel` | `concrete_inherited_rejections_match_source` |
| `fetch_public_trades` | `concrete_inherited_rejections_match_source` |

The additional native HTTP control replaces the retained `PromiseWait` poll with
a fresh temporary waiter per poll. It compiles, reaches the real `/time` request,
and fails the named `native validation timed out` assertion at ten seconds. After
restoration, `polymarket_registry_derives_credentials_and_reads_signed_balance`
passes in under one second. Dropping the temporary waiter removes its wakeup;
this failure is independently visible on Tokio rather than a manually pumped host.

Reproduce from the repository root:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/path/to/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-polymarket --locked
python3 sdk-rs/safe-cex-venue-polymarket/tests/binding_controls.py
cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-registry --test native_host_polymarket --test rust_venues
python3.14 sdk-rs/safe-cex-venue-polymarket/tests/scoped_parity.py
```

Run controls sequentially, with no concurrent test, clippy, or parity reader of
the mutated source. Source-body oracle/control evidence in the existing venue
documents remains separate from these concrete receiver/transport witnesses.

## Tracked shared routing gap

Generic `safeCexAxiosCreate` proxy/tunnel policy remains a shared-runtime FINDING,
tracked by overseer task `ddi2il-12` (codex-l89ogr). Direct supplied-host behavior
is verified here; configured proxy/tunnel routing awaits that shared service.
The overseer explicitly treats this follow-up as non-blocking for this binding.
The native account carrier retains the shared UTF-8 boundary; the venue's retained
UTF-16 `Text`/`Optional` record is unchanged.
