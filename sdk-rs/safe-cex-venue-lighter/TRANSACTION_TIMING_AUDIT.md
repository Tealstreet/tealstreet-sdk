# Transaction client timing audit

The fourteen assigned get/submit rows share the existing native FIFO nonce
queue. Their typed Rust entrypoints already reserve work eagerly and retain
continuations after observers are dropped. Adding another eager driver would
change that family rather than repair it.

The production JS TransactionClient constructor had two observable gaps:
parameter decoders ran before the nonce await, and its method table omitted
getTransferTransaction and all seven submit methods. A nonce rejection could
therefore lose to a parameter getter that TypeScript never reads. The batch
bridge also captured every item before the first await.

The bridge now retains JS parameters until the nonce continuation and calls
the same native builders used by the typed entrypoints. Batch construction
retains the shared iterator record across awaits, including live appended
items and IteratorClose on an abrupt body completion. Submit methods invoke
the receiver's getter immediately and let the canonical Promise driver own
the await/send continuation. The original HTTP client and batch hash array
remain native references.

| Assigned methods | Production proof |
| --- | --- |
| getCreateOrderTransaction / submitCreateOrder | Held nonce, getter failure precedence, live price, detached signing/send |
| getCreateOrderTransactions / submitCreateOrders | Same boundaries plus an item appended during await and one cached nonce allocation |
| getCreateGroupedOrdersTransaction / submitCreateGroupedOrders | Same boundaries through the existing grouped builder and real crypto |
| getCancelOrderTransaction / submitCancelOrder | Same boundaries with a changed order ID |
| getTransferTransaction / submitTransfer | Same boundaries with a changed transfer amount |
| getUpdateLeverageTransaction / submitUpdateLeverage | Same boundaries with changed leverage and the resulting margin fraction |
| getApproveIntegratorTransaction / submitApproveIntegrator | Same boundaries with a changed approval fee |

`tests/transaction-timing-source.cjs` executes the maintained whole TypeScript
class. `binding::signer::tests::transaction_timing_*` drives the production
constructor/globals, FIFO bridge and real Poseidon/Schnorr implementation.
Only terminal nonce/send responses are scripted. There are 28 held-nonce
failure observations, two explicit-nonce approval controls, and 28 successful observed/discarded-handle executions,
covering all fourteen methods independently.

The original typed API still consumes owned parameter records and vectors.
Decision 2 accepts this ownership snapshot boundary; it does not excuse eager
JS parameter reads. `tests/live_transaction_client.rs` retains its independent
41 public cases, seven nonce scenarios and explicit caller-mutation witness.
This finite audit does not claim arbitrary getter reread equivalence inside
all signing builders or JavaScript prototype replacement on the typed API.
