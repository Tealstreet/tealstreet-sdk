# Transport object and string admission

This review is bounded to fourteen concrete decoders and the five map visitors
introduced for the shared frame schema. It does not classify the remaining
transport APIs or aliases, or claim that the full transport accepted-set gate is
clear.

The input contracts are objects for records and strings for literal unions:

| Owners | Reviewed input role |
| --- | --- |
| `HOWRegion`, `LatencyEntry` | `packages/http-over-websocket/src/types.ts` region literals and latency record |
| `HowEgress`, `ResolvedWs`, `ResolvedRouting`, `RoutingOptions` | Resolved routing records in `packages/safe-cex/src/transport/types.ts` and `tealstreet/routing/resolveExchangeRouting.ts`; egress is the resolved `jp`/`sg` pair |
| `HttpRequestArgs`, `RpcHttpResponse`, `ResponseData`, `Config`, `RequestDump` | Shared `infra/tradetunnel/src/frames.rs` protocol records, used by the HOW request/response adapters |
| `ResolvedTransport`, `TransportMetadata` | Account transport resolution flags and exchange metadata projection in `transport_resolution.rs` |
| `SigningHeader` | Existing remote-signing discriminator/header projection in `signing.rs`; the original payload remains buffered as `RawValue` |

The SDK records use the existing concrete `strict_decode::object` frontdoor and
private Serde remote schemas. String enums use `String::deserialize` followed by
the unchanged remote variant schema. Routing defaults, optional-field behavior,
serialized names and opaque latency `Value` fields retain their existing policy.

The standalone tradetunnel crate cannot depend on SDK helpers. Each frame
frontdoor therefore calls `deserialize_map` with its own module-scope visitor;
`visit_map` delegates the original remote schema through
`MapAccessDeserializer`. `HttpRequestArgs<D>` retains `D: Deserialize<'de>` and
borrowed payloads. No JSON-value rebuffer, `DeserializeOwned` restriction or new
shared dependency is introduced. Each visitor remains independently inventoried
and receives its own native entrypoint observations.

`accepted_wire_shapes`, `accepted_frame_shapes` and `header_shape_tests` compare
native fields through raw text, owned values, borrowed values and qualified
Serde calls. Their independently authored complete positional arrays use the
schema's declaration order. The frame test also checks that `HttpRequestArgs<&str>`
borrows directly from its original input. Enum tests compare native variant
identity and exact serialized spelling, then refuse every valid tagged-map form.

Run `python3 sdk-rs/safe-cex-transport/scripts/accepted-shape-controls.py` from the
repository root for fourteen independent compiling guard-disconnect controls.
Each requires its named shape assertion to fail, restores exact source bytes,
and requires that same test to pass. Compilation failure does not count as a
control. Do not run compiler inventory or other source-sensitive probes during
these mutations.


The resolution records preserve optional flag presence and metadata region order.
`TransportMetadata::requires_cors` remains required; no serialization trait is
added to that public metadata carrier. Their two focused guard controls can also
run with `accepted-shape-controls.py ResolvedTransport TransportMetadata`.
