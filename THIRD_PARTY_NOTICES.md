# Third-party notices

This repository is MIT-licensed (see `LICENSE`) except for the components below,
which keep their upstream licences. `scripts/check-licenses.mjs` fails CI when a
`thirdparty/*` directory has no entry here.

## `thirdparty/grvt-ts-sdk`

- Upstream: https://github.com/gravity-technologies/grvt-ts-sdk
- Licence: Apache-2.0, full text in `thirdparty/grvt-ts-sdk/LICENSE`.
- Copyright 2024 GRVT.
- Modified by Tealstreet Ltd: vendored as a workspace package
  (`@tealstreet/grvt-ts-sdk`), with build and lint configuration adapted to this
  repository.

## `thirdparty/grvt-js-client`

- Upstream: `gravity-technologies/grvt-js-client`, published to npm as
  `@grvt/client`.
- Licence: none published upstream (`"license": "UNLICENSED"`). Copyright
  Gravity Technologies.
- Vendored as `@tealstreet/grvt-client`.

## `thirdparty/paradex.js`

- Upstream: https://github.com/tradeparadex/paradex.js, published to npm as
  `@paradex/sdk`.
- Licence: MIT, as declared by the npm package.
- Modified by Tealstreet Ltd: vendored as `@tealstreet/paradex-sdk`.

## `thirdparty/reconnecting-websocket`

- Upstream: https://github.com/pladaria/reconnecting-websocket
- Licence: MIT, full text in `thirdparty/reconnecting-websocket/LICENSE`.
- Copyright (c) 2018 Pedro Ladaria.
- Modified by Tealstreet Ltd: vendored as `@tealstreet/reconnecting-websocket`,
  including HTTP/1.1 ALPN and header-capable constructor support.

## `packages/ccxt-compat`

- Derived from CCXT: https://github.com/ccxt/ccxt
- Licence: MIT. Copyright (c) Igor Kroitor.
- Contains adapted helpers (`Precise`, number and precision parsing) used by
  `safe-cex`.

## `packages/data-structures`

- `src/PriorityQueue.ts` is derived from
  https://github.com/ddurschlag6river/TypedPriorityQueue
  - Licence: Apache-2.0. Copyright the TypedPriorityQueue authors.
  - Modified by Tealstreet Ltd.
- `src/RingBuffer.ts` is MIT, Copyright (c) 2019 - 2022 Dominik Geng; its
  original header is retained in the file.

## `packages/poseidon-crypto`

- `src/wasm/poseidon_crypto.wasm` is a compiled binary of `src/wasm/main.go`. It
  contains https://github.com/elliottech/poseidon_crypto (Apache-2.0) and the
  Go runtime (BSD-3-Clause, Copyright The Go Authors).
- `src/wasm/wasm_exec.js` is Copyright 2018 The Go Authors, BSD-3-Clause; its
  original header is retained in the file.

## zkLink native cryptography and contract serialization

- Files: `sdk-rs/safe-cex-crypto/src/zklink.rs` and
  `sdk-rs/safe-cex-crypto/src/zklink/contract.rs`.
- Upstream: https://github.com/zkLinkProtocol/zklink_sdk at
  `3152525e2615ecbbdb56fd6728ce265ad2dde485`.
- Licence: Apache-2.0; upstream notice retained in
  `sdk-rs/safe-cex-crypto/LICENSE-zklink`. Copyright 2023-present, zk.link.
- Modified by Tealstreet Ltd: extracted deterministic signing and contract
  serialization primitives, with explicit errors for unsafe packing/price
  inputs and a seed-only native signer capability.

## Rust Number radix conversion

- `sdk-rs/safe-cex-exchange/src/js_runtime/number_radix.rs` adapts V8's
  `DoubleToRadixStringView` from Node v24.20.0:
  https://github.com/nodejs/node/blob/v24.20.0/deps/v8/src/numbers/conversions.cc
- Licence: BSD-3-Clause; the full notice is retained in the Rust file header.
- Copyright 2011 the V8 project authors. All rights reserved.
- Modified by Tealstreet Ltd: Rust vectors replace the split character buffer;
  decimal and special values use the SDK's existing number formatter.

## Native JavaScript TimSort

- Source: https://github.com/v8/v8/blob/13.6.233.17/third_party/v8/builtins/array-sort.tq
- Scope: `sdk-rs/safe-cex-exchange/src/js_runtime/sorting.rs`.
- Copyright Python Software Foundation and the V8 project authors.
- Licences: PSF-2.0 and V8 BSD-3-Clause notices are retained in
  `sdk-rs/safe-cex-exchange/licenses/v8-timsort.txt`.
- Modified by Tealstreet: Rust translation of run detection, binary insertion,
  run-stack collapse and galloping merges, using the existing fallible native
  comparator and separate JavaScript receiver boundary.
