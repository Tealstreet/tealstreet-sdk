### HTTP Plan status-refusal scope

The existing default-false status_exempt_refusal hook is consumed whenever
http-plans is enabled, including complete Plans without serial-plans or
session-bearer. Only fully received Unproven responses consult it; ordinary
2xx and non-408 4xx retain their native codec path. The temporary exemption
is scoped to the exact DATA callback and its write classification. A venue
must declare its narrow parsed definitive-refusal predicate before using it.
This prerequisite enables no venue override or transport retry by itself.
