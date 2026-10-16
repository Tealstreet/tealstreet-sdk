# Adversarial classification review

Native sub-agent `review_coinbase` independently selected exactly sixteen
leading-FINDING rows at source pin
`c023025ac423238ec50e19f07d895994967eccc8`. Its read-only review inspected the
source/native bodies and existing evidence, without Cargo, runtime execution,
mutation, production edit or status change.

The reviewer independently reported five behaviour rows: `CoinbaseExchange`,
`CoinbaseSpotExchange`, `CoinbaseExchange.tick`, `CoinbaseExchange.updateOrder`,
and `CoinbasePrivateWebsocket.handleUserEventOrders`. The other eleven are
concrete binding/proof gaps. It specifically identified both JSON-view-only
catch blocks after the native error-carrier repair, and distinguished a missing
ID from the literal string `"undefined"`.

The review requires the local-obligation qualification: the eleven unresolved
proof rows are not proven correct, and transitive class/tick/raw-order defects
remain relevant to their eventual concrete bindings. It also distinguished the
public-only update's synchronous outer error from the discarded inner Promise,
and rejected an invented original-cause return requirement for cancel/place.

This classification closes no row and does not claim five executed failures.
The later integration merge leaves the reviewed Coinbase crate/manifests
byte-unchanged; the final ledger verifier checks exact note and input hashes.

Review of the draft found that category totals alone did not pin the five
behaviour identities. The final verifier adds their independently authored exact
set and the expected sixteen indexes. A count-preserving category-swap control
is recorded alongside the original same-length identity control. This is an
inventory-verification repair, not a sixth behaviour finding.
