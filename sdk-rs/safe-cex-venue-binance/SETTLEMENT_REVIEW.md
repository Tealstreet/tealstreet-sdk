# Settlement continuation contract

`class_jobs::Settlement<T>` owns a shared native future state. Its only selected
API is the actual `Future::poll` implementation; manual Clone construction has a
separate existing review. The accepted-set row classifies the pinned handle and
native task context as typed capabilities. It neither decodes wire data nor
claims a universal generic payload domain.

The independent native binding witness constructs production dependencies and an
actual Futures exchange owner. Its terminal adapter holds the real listen-key
request. Two cloned settlement handles share that one request, remain pending
until release and then both complete without another request. The actual bound
receiver remains usable afterwards.

The direct eager-job witnesses additionally inspect synchronous prefix ownership,
independent task lifetime, both waiter wakeups, original native object identity
and original rejection `Rc` identity. No exchange import or scheduled continuation
is replaced in these tests.

`scripts/settlement-controls.py` independently changes successful completed polls
back to Pending. It requires the real constructed private binding to compile and
fail its bounded-poll assertion, restores the exact source bytes and requires the
same named test to pass. This separates execution evidence from the typed API
classification receipt.

`scripts/settlement-contracts.mjs --expect-unclassified` records the named
missing-contract failure. Its positive mode requires exactly the selected owner,
zero failures and an explicit `reviewed-api` classification. The canonical driver
still performs full compiler and dependency discovery. No raw decoder execution
is claimed by this Future classification.
