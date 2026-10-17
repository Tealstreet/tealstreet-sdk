# Authored JSON inputs in native probes

`loadContracts()` retains JSON source tokens as private metadata. Probe payloads
reuse those tokens for unchanged values. The contract schema, owner census,
validation rules, adapter calls and expected outputs remain unchanged. Generated
multi-entry dispatch transfers input/output tokens to its wrappers. Aliases,
shape audits, supplemental/native observations and free-function probes all use
the same serializer. Modified values are serialized from their new value;
stale source text must never mask an edit.

This preserves authored object order, escaped keys and number spelling before
Rust parses a case. JS still validates the ordinary parsed contract values.
The metadata itself is not an accepted contract field or a discovery exemption.

The regression control removes `js_own_keys` from the actual Aster request
formatter, including its unused import. With the former loader, the selected
18-owner native batch incorrectly passed all owners: JS had already changed
`9,2,x` into `2,9,x`. With source retention, both actual query formatters fail
native output assertions: expected `2=two&9=nine&x=true`, observed
`9=nine&2=two&x=true`. Compiler failures are rejected as control evidence.
`accepted-json-source-native.test.mjs` runs the unchanged source, the real
production mutation, exact restoration and the restored native batch.

Focused tests also retain source tokens through generated dispatch, verify
semantic equality across every loaded production contract, reject stale tokens
after value edits, and execute loader/dispatch metadata-removal controls.
Programmatically constructed or reserialized fixtures cannot recover source
order or numeric spelling they already discarded; see REVIEW-PROPOSALS.md.

Executed validation: the maintained native control passes with 18 baseline and
18 restored production receipts, and two expected runtime failures under the
ordering mutation. The focused loader/contract suites pass 21 tests (including
two executed metadata-removal controls); alias/policy regressions pass 13; CI
wiring passes 368. Complete production serialization preserves the parsed values
of 2,465 rows. The registry retains all earlier tests/mutations and adds five
tests and three controls (145 tests, 176 controls total). Global parity passes.
