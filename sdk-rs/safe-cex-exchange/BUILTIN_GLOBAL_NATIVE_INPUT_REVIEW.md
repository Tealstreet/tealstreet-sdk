# Builtin and global native inputs

The 21 assigned native-input owners now have exact compiler API contracts and
executed native observations. The baseline explicitly reported all 21 as
unclassified. These contracts exercise each real owner rather than substitute
JSON admission for native execution. `tests/builtin_global_input_oracle.cjs`
executes Node builtins and the maintained TypeScript enum declarations; the
installed binding tests independently retain holes, own undefined, UTF16 units,
number bits, referents and thrown identity before serializing observations.

## Behavioral fixes

Four committed, compiled REDs cover Array/Map/Set callback validation,
Array.from mapper validation, Map entry validation and Promise executor
validation. Native previously lost operand text, replaced lone surrogates or
omitted the operand altogether. One `v8_operand` formatter now retains UTF16
and uses the canonical TypeError constructor. It never invokes user coercion.
Primitive diagnostics match Node exactly, including BigInt and Symbol.iterator.
Existing iterator-close and NativeRuntime Promise suites retain their original
identity, first-settlement, getter and ordering assertions.

V8's internal object short-print is the only accepted diagnostic difference:
for an ordinary object with a poisoned own `toString` getter, Node prints
`Promise resolver [object Object] is not a function`; native prints
`Promise resolver #<Object> is not a function`. Both report TypeError without
reading the getter. Plain objects and arrays match the observed Node strings.
This internal display distinction does not authorize coercion, a lost rejection,
or a different primitive message. The installed residual test pins both strings
and zero getter reads. The shared known-residuals ledger records the same ruling.

## Owner boundaries

| Owners | Runtime observation and classification |
| --- | --- |
| array_length, indexed, items | Intrinsic Array storage, captured length, holes versus undefined and original references. ACCEPT is confined to the actual Array producer domain. |
| flatten_into, flatten_length | Installed flat at depths zero/one, sparse nested rows, -0, UTF16 and object identity. No arbitrary array-like length claim. |
| map_entries | Actual Map.set/get and identity of its storage handle. The non-Map branch is unreachable through the intrinsic selector. |
| member, is_regexp | Installed receiver/member and real RegExp dispatch; unknown UTF16 names do not alias ASCII members. The regexp helper identifies the physical native brand. |
| require_callback, array_from, map_from_entries, not_iterable, v8_operand | Exact source diagnostic FIXes plus successful native traversal and referent preservation. |
| iterate, consume_iterator | Optional iterator acquisition, raw values, original thrown identity; existing installed Array.from tests pin inherited methods/getters and abrupt close order. |
| chain_value | Actual NativeRuntime finally continuation preserves fulfillment/rejection reference after cleanup. |
| construct_promise | Exact invalid primitive executor diagnostic FIX plus existing native constructor suite for synchronous invocation, first settlement and thenable getter identity. |
| install_builtins, install_enums | Fresh namespace composition, retained unrelated sentinel, executable static imports and maintained enum values. No arbitrary frozen-global mutation claim. |
| json_stringify, stringify_json | Canonical live-holder serializer: UTF16, own undefined omission, nonfinite values, -0 and original getter throw. Existing installed replacer/toJSON/cycle tests remain unchanged. |

The receipt cases are also checked against fresh Node execution in the registered
`js_runtime` target, so source observations cannot silently drift from contracts.
Only the two probes that instantiate NativeRuntime require the native feature;
all other owners remain measured under their ordinary package feature scope.
