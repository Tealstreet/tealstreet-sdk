# Bitget uncertain protective cancel confirmation

A Classic empty TP/SL acknowledgement remains Unknown. UTA's documented success
with `data:null` stays successful. No write is replayed and no dependent
cancel/replace is allowed to proceed on an unproven cancellation.

Native sources register at most 64 TP/SL native targets, with 128-byte identity
bounds and a 120-second check lifetime. Checks capture a monotonically increasing
registration serial. A census created before registration cannot answer it;
an in-flight census owes one coalesced full successor. Expiration proves nothing.
A clean, complete, correctly scoped and covered post-registration census reports
`bitget.cancel.confirmed` present or absent before cached-algo retention. Absent
means not listed by the read, never that the cancel caused it. Existing terminal
rows stay terminal; no Canceled status is fabricated. Failed, partial, wrong-mode,
stale or unprovable reads leave the check unproven. Native retry, REST pauses and
read-cover admission remain in force. Only confirmed target absences bypass
cached-algo retention; unrelated protections keep their existing behavior.

Frame-fed SDK callers must deliver `orders.cancel_check` before the existing
`orders.refresh` control. Targets travel on that same `refreshAccountOrders`
call, whose source first runs feature-gated read-only `registerCancelChecks`,
then `refreshOrders`. Registration creates no request, result proof or write
capability. A registration refusal is surfaced but cannot suppress the ordinary
repair read. Both source and writer need this SDK/wasm change for confirmation;
the generic refresh remains the prerequisite #3065 contract. Control notices
survive diagnostic log filters. Engine-retired HTTP failures retain targets too:
a feature-gated read-only venue getter hands them to the same source ask before
it emits. The retirement cleanup hook retains its existing lack of Cx authority.
No extra source ask or placement-hold timer is
introduced.

Diagnostics retain bounded mode, endpoint class, HTTP status, numeric native
code, data-shape class, list counts and requested/present/absent/unproven state.
The original native message is unchanged. No raw response, header, secret or
URL is included. `none-or-unrecognized` deliberately does not claim that the
raw native acknowledgement contained empty lists.

Production controls retain the original 3 assertion REDs committed before the
source change. They cover fresh absence versus cached retention, fresh presence,
and an old census plus exactly one fresh successor. Added controls cover actual
frame-fed target delivery, failed/paused/partial/wrong-symbol reads, UTA null
success, read-only registration and a newer filled row. The upstream #3057 read
cover controls and existing cancel/amend contracts remain unchanged.
