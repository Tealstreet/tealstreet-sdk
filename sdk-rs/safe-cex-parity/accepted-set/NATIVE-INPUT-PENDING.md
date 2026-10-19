# Newly discovered native inputs

`new-native-input-owners.json` records the 149 previously undiscovered input
owners exposed by #1193: six in AsterDEX and 143 in exchange core. These rows
are pending classification, not accepted divergences or measurement exemptions.
The full discovery report is in
`safe-cex-exchange/evidence/retained-handler-carrier/native-input-discovery.json`.

The driver validates the complete compiler inventory before applying an explicit
measurement scope. An unlisted, unclassified native-input owner fails the gate.
A listed owner must remain present until a reviewed removal ruling is recorded.
Adding its ordinary contract requires changing its state to `classified` and
recording the ruling; deletion requires `removed` and a ruling. Normal contract
and native observation checks still apply. The ledger never filters inventory
or supplies decoder or alias permission.

Master integration adds three separately counted pending rows introduced by #1238:
`structured_clone`, `copy`, and `hidden` in
`safe_cex_exchange::js_runtime::structured_clone`. Their owner is codex-0xekzz.
The original cohort remains 149; that integration total is 152 (146 core, six AsterDEX).
These additions grant no measurement permission and use the same exact-owner gate.
The three clone rows are now classified: structured_clone/copy have ordinary
executed native graph contracts and hidden is the internal descriptor writer.
Node-backed evidence and the single approved stack-representation residual are
recorded in safe-cex-exchange/STRUCTURED_CLONE_REVIEW.md.

The #901 response-carrier cutover (`84b06aec6f`) also introduced the native
`safe_cex_venue_apexomni::api_runtime::ApexApi` response input, discovered by
#1193 and first reached by #848 run 39216880873. Its exact pending row is owned
by codex-0w5ohi. That integration total is 154 (146 core, six AsterDEX, two ApexOmni);
the original 149-owner cohort remains separate. An ordinary contract and native
binding receipt are still required; the row grants no measurement permission.

The complete Apex inventory also exposes `binding::http::carry_config`, unchanged
since master commit `251d9906dea` before #901. Its separate pending row is owned
by codex-0w5ohi, with the same ordinary-contract and native-receipt obligations.
The census found no other unlisted Apex native-input owners.

The #1248 Store graph cutover (`94c1cdc90d`) introduced three core native-input
owners in `store_payload`: `sanitize_for_post_message`, `StorePayloadPreparer`
(`prepare`), and `TickerBaseline` (`copy_from`, `pack_deltas`). Complete core
compiler inventories independently identify exactly these three unlisted owners.
Their separate pending rows belong to codex-4ri6pb; the original149 cohort is
unchanged. The total is157 (149 core, six AsterDEX, two ApexOmni). These rows
record outstanding classification and native-binding receipts, not ACCEPT rulings;
normal measurement still requires their contracts. No inventory guard is relaxed.
