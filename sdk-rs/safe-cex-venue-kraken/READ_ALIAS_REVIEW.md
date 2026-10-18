# Kraken native read aliases

The production `ReadHost` implemented by `Scope<KrakenExtra>` binds `Value = Js`
and `Error = Thrown`. Its three public aliases are reviewed only at this concrete
host (`ReadResult` additionally at `T = bool`). The fixed registry pins the fresh
compiler generic declarations, complete raw/expanded targets and source identity;
independently authored native expected types retain `Rc<Js>`,
`Result<bool, Thrown>` and `Vec<(Vec<u16>, Rc<Js>)>`. Actual compiler artifacts
resolve every external alias root before finite selection.

`Handle<Rt>` is an owning reference to a native value rather than a wire decoder.
`ReadResult<Rt,bool>` is the native completion/error carrier rather than a decoder.
`Entries<Rt>` retains own UTF-16 property keys and native value references; it is
not a JSON record decoder. Ordinary Serde and all maintained conversion detectors
still execute for each finite alias; other hosts, result payloads and aliases
remain unclassified until separately reviewed.

The actual constructed Futures/Spot Scope witnesses observe alias types before
serialization: object referent identity and later mutation, original thrown
error identity, UTF-16 units, signed zero and infinity, own-key order and getter
counts. Named semantic controls alter one concrete ReadHost consumer at a time
and require a compiled assertion failure followed by exact-restored success.
No callback mock or trait implementation replaces the host under test.

The existing `binding::rt` module becomes visible only within this crate so the
canonical alias probes in the declaration module can name the actual production
host. No SDK-public type, constructor or behavior is added for the probe.

The missing-contract replay produced three actual named runtime failures before
classification. All three constructed-Scope tests pass; Handle/keyed,
ReadResult/field and Entries/entries semantic faults each produced a named
compiled assertion failure and an exact-restored pass. No compile failure was
counted as a negative control.

After these three profiles, the canonical complete Kraken inventory clears all
25 receivers and these three aliases, then stops on the separate
`private_accounts::PageFuture<'a,H>` fixed-future shape requirement. That is the
next cohort; this PR makes no full-inventory or remaining-21-alias claim.
