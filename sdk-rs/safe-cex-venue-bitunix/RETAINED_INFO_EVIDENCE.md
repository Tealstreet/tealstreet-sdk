# Retained model-info observations

The venue model and private-message hosts now use canonical `RetainedJsonInput`.
Typed Store edges use the shared native codecs. The completed HTTP request alone
uses the existing bounded wire adapter; info readers never construct a JSON view.

| Path | Native observation | Compiled control |
| --- | --- | --- |
| `native_identity::retained_model_info_survives_real_amendment_without_json_projection` | Real cancel/replacement preserves NaN, negative zero, own undefined and original UTF-16 metadata in arguments and Store | Previous whole-model JSON hop fails; direct native edge passes |
| `private_binding_frames::private_position_frame_preserves_retained_store_info` | Real private frame patches position ID while retaining opaque Store metadata | Previous private model host fails; retained host passes |
| Four `native_identity::retained_standalone_*` cases | Real amendment requests preserve nullish ID precedence, UTF-16 trigger/execution-kind fallback and undefined sibling separation | Each old branch compiles and fails its named request assertion; restored branches pass |
| `private_binding_frames::private_order_update_retains_utf16_info_client_id_when_cache_misses` | Real UPDATE reads the original UTF-16 client ID, misses an ordinary String cache and applies the source fallback | Old UTF-8 intermediate stops the patch; restored native lookup passes |
| `private_binding_frames::private_malformed_json_keeps_source_null_parse_behavior` | Invalid text maps to source null, without errors/reconnect; next valid frame arrives | Throwing parse adapter causes the named error/reconnect assertion to fail; source-null behavior passes |

All six assigned crates compiled their test consumers. The corrected source
checkpoint `f36dbb3e10` passed strict native Clippy for all targets in Bitget,
Bitunix, GRVT, Kraken, MEXC and WEEX, serially with one Cargo job. Original order
model, event composition, order-link/state and TP/SL source corpus floors remain.
The expanded TP/SL corpus observes 1,194 native ID results/errors and 102 explicit
final String-output refusals, all 72 trigger calls and all 648 reduce-only calls.

The 250-case order-link corpus retains three raw-ID String-output refusals while
all other helpers execute 250 native comparisons, 278 parent rows and 18 signed
zero observations. Info now admits the original units; these refusals belong to
returned String IDs. Typed top-level IDs and the String-key cache are separate
shared boundaries. No executable-object or alias-identity widening is claimed.

## Unchanged package debt

The official compiler inventory also reaches existing JS-only hosts without
records in the prior contract shard. Their source signatures and bodies are
unchanged by this retained-info work. They are recorded individually rather than
covered by a blanket contract or exemption:

- `adapter::BitunixVenue`: runtime constructor and existing Exchange overrides.
- `sockets::Conn`: live connection state, socket creation and listener operations.
- `sockets::Feed`: public subscription handler/ticker/book operations.
- `sockets::Protocol`: inherited transport-close/error callbacks.
- `order_placement::message` and `order_placement::row_field`: existing JS placement
  message/field projections.
- `order_models::is_unsupported_order_operation`: existing String-only error
  substring predicate.

Changed retained producers, readers and hosts receive individual native contracts
and exact compiler API records. Existing decoder observations are retained when
migrating their adapters; the unchanged debt above is not a passing-gate claim.
