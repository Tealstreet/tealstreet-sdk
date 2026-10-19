# Live socket handler ownership

The recorded OKX business candle invoked its subscription while Base statistics
reported `hadHandlers=false`. Registration lived in the JS `messageHandlers`
object; the typed Base table was empty. Public dispatch separately rebuilt a
typed mirror of the entire JS table on each frame. That evaluated unrelated
getters and captured callback lists independently of later JS mutations.

The production socket binding now reads the selected JS key directly for public
and business dispatch. The captured callback referent is iterated through the
canonical iterator, preserving live list changes, abrupt completion and original
throws. Public frame routing invokes the live dispatch member. Book sequence
acceptance still precedes statistics and callbacks. No callback mirror or mutation
barrier remains, and `sync_handlers` is deleted.

The shared JS socket host installs a selected-key statistics reader once when
creating the Base core. Record, pong, cleanup and reconciliation use the existing
core algorithm with that reader. The separate native registry installers remain
installed after later core accesses. No public protocol trait changes.

The sync-path census found only `safe-cex-venue-okx/src/binding/sockets.rs`:
OKX, OKXDemo, OKXEU and OKXEUDemo share these constructors. Public and business
registries both use the live reader; private bare protocol signals keep source
defaults. Pure typed sockets still own their Base registry directly. The lower
adapted public socket helpers retain their separately declared receiver bounds;
they are not an alternative production JS dispatch path.

The original tape SHA256 is
`39615880a701fea83d1b66e9b14fe15f118a8738f36e8fdb15478bf3debf49fd`.
`tests/recorded-candle.json` pins its candle payload. The production binding
witness exercises two subscriptions, removal of one parser, final removal and
a late frame. The 105-frame napi prefix ends after the first ticker publishes
the candle statistics; this bounded proof makes no full-minute coverage claim.

Binding and maintained-source controls cover direct assignment, defineProperty,
deletion, prototype lookup, unrelated poisoned getters, selected getter failures,
live callback-array growth after key deletion, callback throw identity, pong,
forced cleanup and periodic reconciliation. The pre-cutover public lookup fails
the unrelated-getter control; the source-table dispatch passes without reading
that property. Getter failures retain the original thrown referent before writes.

Run `python3 safe-cex-exchange/scripts/js-handler-registry-controls.py --logs ABS_DIR`
from `sdk-rs` with the normal oracle dependency environment. Each independent
statistics, table-enumeration and snapshot-iteration fault must compile, fail its
named real binding assertion, then pass after byte-exact source restoration.

The strict 105-frame napi candidate fixes the emitted candle statistics; all
three emitted values and own-property presence match the maintained source and
both implementations repeat. The final native Store still omits the source's
own-undefined `starkConfig`. All 2041 pre-existing key-list differences (including
that presence difference) are pinned unchanged before/after. These are separate
Store/model obligations; no ordered whole-output or full-state parity is claimed.
