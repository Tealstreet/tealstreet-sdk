# Default Axios query parameters

`query::axios_default_query` follows installed Axios 1.20.0 `buildURL` and
`toFormData` defaults. Both the native terminal's JSON params path and the shared
JS HTTP host use it. The JS host renders URL-consuming params before JSON
projection so NaN and infinities reach text conversion. Explicit serializers keep
ownership; HOW adapters retain their JSON frame params.

Native tests assert nested bracket paths, flat arrays, array-like objects,
metadata JSON, whitespace, numeric own-key order and exact encoding. The source
oracle independently specifies 15 cases and checks both `axios.getUri` and actual
local-server request targets:

```sh
node sdk-rs/safe-cex-transport/tests/axios-default-query-source.cjs
```

`safe-cex-exchange/tests/native_default_query.rs` sends actual HTTP requests through
the production JS host and native terminal. It checks NaN, both infinities,
negative zero, omitted undefined/null, punctuation and nested keys. The old native
encoder failed six cases; the old host sent only `z=0` for the nonfinite fixture.

The renderer preserves literal apostrophes, like Axios. The physical reqwest
0.13.5 backend normalizes one to `%27` through `url::Url`: source sends
`/witness?a=x~y!z(1)'`, native sends `/witness?a=x~y!z(1)%27`. Reqwest exposes no
raw-URI request override. A different backend needs independent review of TLS,
redirect and timeout behavior. This residual remains explicit; no passing test
asserts the divergent encoding. The source oracle retains the apostrophe case,
and native renderer tests assert the exact source output.
