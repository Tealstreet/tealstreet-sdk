# OKX write planning

Placement builds and validates every final ordinary, trigger, trailing and TP/SL
payload before admitting the first write. This includes every split lot, every
row across the 20-order batch boundary and every per-lot protective child. A
known-invalid element refuses the whole command as `InvalidInput`/`NotSent`
without a transport request; it never leaves a valid prefix to execute.

The pure `validate_write` gate takes each planned write and its captured native
rules, returns a typed error, and checks both row values and native batch count.

The validator checks the serialized quantity and prices against native lot/tick
precision, `minSz` and the applicable limit, market, stop or trigger size cap.
Native sizes stay in contracts, separate from the public market minimum scaled
by contract value. Missing per-type caps retain the existing conservative
instrument cap. These SWAP/FUTURES instruments declare a minimum contract size,
not a separate minimum-notional filter. The field definitions are in the
[OKX instruments API](https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments).

Rounding, caller IDs, reduce-only flags and valid batch/protection ordering are
preserved. The planner refuses an invalid split tail rather than resizing lots.
Explicit zero-size SL/TP still uses `closeFraction=1`; positive sizes cannot use
that exemption, and trailing stops still require an explicit size. Ambiguous
released writes retain the existing no-resend and reconciliation behavior.
