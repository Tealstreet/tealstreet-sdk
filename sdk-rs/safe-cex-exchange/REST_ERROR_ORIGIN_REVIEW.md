# Typed REST rejection observations

`JsRestAdapter::execute` previously retained only message/code in its existing
`OwnedCallbackError`. Converting the resulting `RestFailure` to `ExchangeError`
therefore changed AxiosError/TypeError into Error and omitted stack, cause and
Axios flags. Aster's constructed syncPositionMode and WOO's constructed signed
client both compiled and failed their native name assertion before the fix.

The adapter now captures name/message/code/stack/cause/isAxiosError into that
same owned origin. Nested native Error causes retain their non-enumerable
metadata. The typed error keeps its stack/Axios flag, and materialization walks
the existing source chain to restore native fields and the nearest retained
response. #1055 remains the only response authority; no cached JSON response,
local Rc transport handle or parallel rejection carrier is introduced.

The maintained-source oracle executes the whole Aster/WOO classes with terminal
Axios adapters. Aster rethrows a no-response AxiosError; a response error follows
its distinct formatted Error branch. Native assertions cover Error class,
name/message/code/stack/Axios flag, nested Error name/message, infinity,
own-undefined, response status/data and WOO's real signature header.

The overseer accepts original-object/prototype identity as an owned-tree
residual under shared decision 2. Materialization recreates the Error class and
data; it does not claim to preserve live references. Unsupported structured
causes retain the existing visible owned-value refusal.

The execute cancellation pin was reviewed against exported production syntax.
Capture remains synchronous after the existing await; no task, waiter or owned
reservation is introduced, so its no-owned-latch classification is unchanged.
