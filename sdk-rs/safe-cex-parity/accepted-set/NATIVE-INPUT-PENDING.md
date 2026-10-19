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
