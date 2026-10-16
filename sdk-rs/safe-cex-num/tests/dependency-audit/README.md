# Collation dependency audit

Baseline: `88214fcc06a0bd59ca86301d247c9f953514c49b`. Metadata and feature trees
were resolved for all members without building the workspace. `changes.json`
records the rejected ICU4X 2.1.1 proposal; `proposed-icu.patch` reproduces it.
It downgraded six existing packages and changed 19 packages overall. It is not
applied to the shared graph. `newer-changes.json` records ICU4X 2.3.1: no existing
version changes, 14 feature expansions. Extra data pins do not select older data.

## Existing-consumer impact review

The existing ICU dependency route is URL → IDNA → `idna_adapter`. Its `Adapter`
stores `Uts46MapperBorrowed` and `CodePointMapDataBorrowed`, initializes baked
static data, and invokes character-iterator normalization. Sub-agent source
review found no changed behavior on that route. This is narrower than claiming
that every feature merely adds APIs:

| Package | Added feature and effect |
| --- | --- |
| icu_locale_core | alloc also widens `extensions::unicode::Value::try_from_utf8`: a third subtag becomes accepted. Existing IDNA never parses locale values. |
| icu_normalizer | utf8_iter/utf16_iter add input APIs; IDNA continues using unchanged character iterators. |
| icu_provider | alloc adds owned payloads and changes their carts/auto-traits; IDNA uses borrowed baked payloads. |
| litemap | alloc adds Vec-backed maps and ownership conversions; explicitly selected existing storage retains sorted-map semantics. |
| potential_utf | alloc/writeable add ownership conversions and TryWriteable, not a decoding policy. |
| serde | alloc is redundant for existing collection behavior with std already enabled. |
| smallvec | const_new adds construction; union changes representation and unsafe internals, retaining sequence semantics. |
| stable_deref_trait | alloc adds trait implementations for owned containers. |
| tinystr | alloc adds owned integrations and comparisons; existing parsing is unchanged. |
| writeable | alloc adds owned output; sink-writing behavior is unchanged. |
| yoke | alloc adds owned carts and allocation-backed implementations. |
| zerofrom | alloc adds String/Cow conversions. |
| zerotrie | alloc adds owned construction; borrowed lookup is unchanged. |
| zerovec | alloc changes storage/drop machinery and adds owned mutation; borrowed parsing/lookup retain semantics. Its serde alternatives are not activated. |

## Reproducing the URL experiment

Run `python3 sdk-rs/safe-cex-num/tests/dependency-audit/run-url-audit.py PROFILE`
with PROFILE `before`, `rejected`, or `newer`, and cargo/rustc on PATH. The script
archives the pinned baseline under this worktree's ignored target directory and
builds only the transport oracle with two jobs. It tests the real exported
`extract_domain_from_url` against live Node, including IDNA, soft hyphen, IPv4
numeric spellings, mapped IPv6, non-special schemes, and U+FEFF. All three profiles pass the expanded 57-case corpus; result logs are retained.

## Collation finding

The generated Unicode corpus distinguishes Node's Unihan radical-stroke root
from ICU4X's default implicit-Han root: U+30D2F sorts before U+9A68 in Node, but
after it with both ICU4X 2.1.1 and 2.3.1 compiled data. Version pins alone do not
pin that data-generation choice. ICU4X source exposes `CollationRootHan::Unihan`;
the primitive must use that profile rather than the default baked root.

The final graph (`final-*`) has the identical 14-feature diff, zero version changes.
The scoped URL audit uses historical transport source so future changes do not
silently change this experiment. The proposed patch is evidence, not a dependency
recommendation. Generation build logs are retained as provenance, not CI gates.
