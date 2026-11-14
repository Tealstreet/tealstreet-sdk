# DELEGATED restart recovery

The server keeps delegated signing material and algo actors in memory. A new
process cannot reconstruct either from a router interruption marker. This
extension reuses the authenticated interruption header, its bounded notice and
restore ledger, the existing key handover/probe, and the client TWAP restore
operation. It adds no key, schedule or order persistence.

## Status contract

An authenticated `delegated-key-req` for an unjoined account with an unexpired
router interruption returns the existing `delegated-key` offer followed by:

```json
{"t":"msg","d":{"op":"delegated:status","accountName":"account","data":{"status":"waiting-for-key","reason":"waiting-for-key","message":"waiting for your key"}}}
```

When multiple DELEGATED venues are enabled, an unjoined caller must include
`data.exchangeName` in `delegated-key-req`. An unresolved or disabled venue is
refused before any key offer or waiting status; the interruption marker remains
intact. An explicit enabled venue permits the normal offer/status sequence.

This is connection admission status, not a job inventory or proof that all
venue orders were canceled. It discloses no key, prior account settings or algo
IDs. Existing `owner-released` and `lease-expired` router markers qualify; the
marker parser also accepts the additive typed reason `waiting-for-key` using
the same shape, authentication, limits and retention. An offer alone never
admits an account or consumes the interruption/restore fence.

Only a successfully admitted DELEGATED handover emits the same op with
`status: "key-available"` and `message: "your key is available"`. The reason
continues to identify the recovery condition. The output carries the admitted
init registration's `instanceId`, when supplied. Pending, refused, retired or
other-user proof completions cannot emit it. It does not assert that algos are
running or acknowledge any restored job. Retrying the key request can repeat
the waiting status; the existing forced notice remains one-shot per user.
Successful DELEGATED admissions confirm key availability even if the marker
expired during the proof or was absent. LOCAL/COSIGNED envelopes and the
existing handover offer bytes remain unchanged.

The router already owns the durable interruption record and strips untrusted
client headers; this SDK change does not introduce a second router ledger.
Router restart detection/emission and web/mobile rendering are separate rollout
work: a reconnect without a router marker cannot infer a restart from a missing
account alone. A consumer should keep a client-held schedule visible while
waiting and offer restore/check/resume after key admission, without claiming a
lost server job still exists.

## What can recover

| State | Survives a box process restart? | Recovery |
| --- | --- | --- |
| Delegated key copy | No | Fresh handover and venue admission |
| Server TWAP actor and progress | No | Only a client-held `twap:state` copy can restore it |
| Server chase/EXEC actor | No | No restore operation; reconcile orders before new work |
| Existing venue orders/positions | Owned by the venue | Fresh account snapshot; never infer cancellation from restart |
| Router interruption marker | Router-owned metadata | Existing authenticated header and bounded restore fence |

Restored running TWAPs are forced paused by the existing interruption guard,
with stable IDs and progress retained. A successful explicit `twap:resume`
acknowledges only its user/account/job and observed marker/lease epoch. Merely
providing a key does not acknowledge, resume, replay or duplicate a job. A new
interruption re-arms the guard. Without a client schedule copy there is nothing
to restore; a status must not fabricate a chase or TWAP row. No automatic
cross-process algo resume is promised.
