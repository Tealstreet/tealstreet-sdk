# Writer RPC

A writer is a `writesOnly` engine: it holds a real key, opens no socket, makes
no read request and keeps no venue state between commands. Each request is one
venue write. The caller builds its `context` (orders, positions, per-symbol
settings) from the store its stream side keeps, and the writer applies that
context to a scratch store for that one command only.

`writer-rpc.schema.json` is the wire contract and `fixtures.json` its canonical
examples. The safe-cex encoders (`packages/safe-cex/src/writer`) and the engine
(`engine/tests/writer_rpc_fixtures.rs`) both test against the fixtures, so any
host can serve the same requests.

## Runtimes

Three runtimes exist side by side. None of them replaces another.

| Runtime                | Streams and algos        | Manual writes                   | Algo writes                                    |
| ---------------------- | ------------------------ | ------------------------------- | ---------------------------------------------- |
| Browser worker         | the browser's own engine | browser writer, direct to venue | browser writer, through the same RPC           |
| Hosted, remote signing | exchange server          | browser writer, direct to venue | server engine; the browser signs each sign-req |
| Hosted, keyed          | exchange server          | browser writer, direct to venue | server-hosted writer, through this RPC         |

In all three, manual writes go browser-direct, on one network leg. Where the browser writer runs (main thread, stream worker or
a dedicated writer worker) is a host choice, made by measurement, and does not
change the contract.

## Messages

- Request: `{kind: "writerRequest", id, account, command, context, marks?}`.
  `command` is an engine host command without a completion id. `context` is
  required, and may be `{}`.
- Reply: `{kind: "writerReply", id, ok, result | error, marks?}`. `error` is a
  safe-cex `SerializedCommandError`; `outcome: "unknown"` means the write may
  have executed.
- `marks` carries the `write.timing` stage stamps across a thread or process
  hop.
