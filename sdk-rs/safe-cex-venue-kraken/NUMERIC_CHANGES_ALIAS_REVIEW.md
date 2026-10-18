# NumericChanges borrowed decoder and live publication

`stream_updates::NumericChanges` is exactly `Vec<(&'static str, f64)>`.
It implements borrowed `Deserialize<'static>`; it does not implement
`DeserializeOwned`. Its empty sequence can also be read from an owned Value,
while a nonempty owned Value cannot supply static string keys. This is a real
decoder, even though production builds patches from source literal keys.

The exact-owner policy in `accepted-kraken-numeric-changes.mjs` authenticates the
compiler declaration, raw/expanded Vec/tuple/ref/f64 shape, no generics, sysroot
Vec artifact and verified serde_json origin. Its invariant native expected type
is independently authored in that policy. It grants no permission to another
alias or shape; the general borrowed refusal and owned decoder bound remain.

The maintained alias runner executes 68 cases: static JSON text, qualified
static text Deserialize, borrowed static Value and owned Value. Static text
lives in the program; borrowed Values live in OnceLock, without leaking or
fabricating lifetimes. Native outputs check ordered duplicate keys, each f64 bit
(including -0 and the minimum positive subnormal), string slice offsets or
borrowed Value pointer identity, empty arrays and malformed tuple/primitive
refusals. Escaped text keys require allocation and are refused by the borrowed
text route; borrowed Value keys remain admitted. All alternate trait detectors
execute separately and must remain empty. No JSON roundtrip observes numbers.

`accepted_numeric_changes` separately drives actual constructed Futures and Spot
public sockets into DefaultStore. Each family has distinct values for every
published numeric key, signed zero and untouched-field assertions. Existing
`production_ticker_coercion_{futures,spot}_matches_constructed_typescript` binding
cases independently compare the unchanged maintained TypeScript classes and
DefaultStore. The new fixture does not substitute a fake TickerUpdateHost.

Run `numeric-changes-contracts.mjs`, the registered parity policy/native tests,
`--lib accepted_numeric_changes` and those two source/binding regressions.
`numeric-changes-controls.py` must compile two independent wrong-key publication
faults, require their named native assertion failures and restore exact source
bytes before passing again. The parity controls additionally corrupt native
identity, native member order and route dispatch; each must produce a named
runtime receipt failure followed by a restored pass.

This closes this alias's finite borrowed decoder coverage, not arbitrary static
borrowing or every Kraken runtime behavior. Full canonical inventory remains an
independent gate and must report any later obligation honestly.
