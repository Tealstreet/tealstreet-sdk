# Read-only adversarial audit review

Native sub-agent `review_coinbase` independently inspected all seventeen
previously non-FINDING divergent annotations against source/native bodies,
existing witnesses and the canonical references. Its final disposition was
eleven supported explanations and six incomplete explanations. The preceding
sixteen-FINDING review independently established five behaviour and eleven
rationale/proof rows. Combined disjoint totals are eleven supported, eight
behaviour and fourteen rationale/proof, covering all thirty-three identities.

The reviewer identified the REST mapper's raw-info clone, the Buy/Sell wrappers'
deep clones versus source shallow spread, missing exact returned-unit
attribution for credential parse/normalization, and the missing lone-unit
TextEncoder/base64url witness. It distinguished direct PlaceOrder's borrowed
intent from the two owning wrappers. Those six notes are now explicit findings.

It also found two repairable annotation problems: obsolete shorthand on the
label validator, whose dedicated source/native witnesses do exist, and the
false claim that serde_json::Map sorts keys despite preserve_order. Those
underlying explanations remain supported after correction.

This was static source/native and existing-witness inspection only: no Cargo,
new oracle execution, production mutation or new runtime failure was measured
by the reviewer. Complete audit coverage does not establish complete-function
parity or close any of the twenty-two remaining findings.
