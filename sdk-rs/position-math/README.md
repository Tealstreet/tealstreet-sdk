# Position value policy

This dependency-free crate owns native position-size evidence selection and
unknown-preserving notional/leverage arithmetic. The engine, hosted EXEC and
numeric contract tests use the same implementation. It exposes no account,
transport, store or scheduler; algos still reach those through AccountPort.

Unavailable or conflicting native size returns `None`; unavailable valuation
and incomplete or overflowing exposure remain NaN. Reported zero stays zero.
