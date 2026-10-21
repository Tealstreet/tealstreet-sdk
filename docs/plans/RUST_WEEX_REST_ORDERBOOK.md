# WEEX REST order-book parity

Fresh actual landing: `d510b9813235f7bf99c6eac9c630a583fb0c1212`.
Plan first; independent venue batch with only reviewed Backpack #1680's precise
standalone book decoder as a genuine prerequisite. Preserve actual KCS rollback,
WEEX metadata #1675 and command-owned empty-fill markers #1670.

1. Reproduce missing native capability/JSON decoder and actual query dispatch/
   owned completion (current command emits unsupported notice without settling).
   Match unsigned GET /capi/v3/market/depth?symbol=...&limit=N at the existing
   API root, raw encoded symbol without market lookup, default200/explicit limit.
   WEEX has no supported Rust testnet switch or new product enablement.
2. Add independent Task::BookQuery and unstreamed parser. Reuse existing get()
   shared-pause admission, route response before generic failures while retaining
   HTTP429 rate::delay pause, and settle deferred/transport ownership exactly once.
   Do not touch subscription depth generations/buffers or account/history logic.
3. Match null-coalesced bids/b and asks/a aliases, missing/null sides empty,
   native order/duplicates/zero quantities, cumulative BASE amounts/no IDs or
   contractVal scaling. Preserve exact integer/string lastUpdateId; missing/null
   becomes empty string. Keep subscription wire::Depth strictness unchanged.
4. Deliberately correct TS's permissive malformed-row numeric fallback/prefix
   parsing: reject malformed/nonfinite finances and cumulative overflow atomically
   instead of reporting corrupt/partial success. Require actual object/array
   shapes; ignore tuple metadata after its first two numeric fields.
5. Accept absent native code and code0/200; public native/HTTP/read failures
   retain structured code/message and typed NotSent/non-auth kinds. Reuse native
   shared429 policy (rate::delay, bounded60s and header/window precedence), no new
   retry loop. Native get() currently does not auto-retry failed requests; keep
   that policy rather than copy TS's three-network/one-429 retry mechanism.
6. Enable precise JSON decoder/capability and align combined-host/SDK bridge
   fixtures. Regress exact URL/limit/noseed, large version/base totals/aliases,
   malformed/empty responses, reverse concurrent owners/duplicate callbacks,
   HTTP/native auth-looking refusals, shared pause/deferred command ownership,
   and existing live book/account/metadata independence with next WS continuity.
7. Independent plan/implementation reviews, WEEX+Binance native gates, default
   combined-host capability test, focused SDK test/typecheck/lint, strict
   Clippy/fmt/bans, full final CI and fresh separate guard before actual merge
   verification/overseer and caller receipts. No local release/full replay builds.


Implementation evidence:
- Eight original-source groups fail through actual standalone decoder/native
  dispatch, then pass. Exact default/custom-limit URL, null-coalesced aliases,
  huge raw integer/string version, native order/zeros/cumulative base amounts,
  strict financial/overflow rejection, public native/HTTP/read failures and
  reverse concurrent/duplicate ownership are pinned.
- Shared429 Retry-After precedence defers a new command on its own timer and
  completes only that owner after dispatch, with no failed-owner retry.
- Seeded tiny-contractVal/live depth tests prove query base quantities remain
  unscaled; REST success/native auth-looking failure cannot change live book,
  ticker/account state or sequence. Next WS delta and an outstanding query across
  unsubscribe/resubscribe preserve independent query/subscription generations.
- Final local gates pass:416 WEEX+Binance native tests including existing
  metadata/settings/history coverage; default combined-host capability fixture,
  22 SDK bridge tests, typecheck, explicit no-ignore ESLint and strict venue
  Clippy/format/bans. Two independent final implementation reviews CLEAR.
  Full exact-head CI and current landing integration/separate guard remain.
