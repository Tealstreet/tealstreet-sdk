# Keyless public-data reads via the public proxy on any non-direct account

**Change.** A keyless public-data-only engine (a worker mirror's chart source) on an
opted-in venue now takes the public CORS proxy whenever the account is not plain
direct: `tealstreetProxy` or `tealstreetTunnel` set. Before, only `tealstreetProxy`
qualified.

**Why.** A tunnel-only account's chart source has no tunnel on MEXC or BingX
(`usesTunnel` is false there under public data only), and `restOptionsFor` drops the
tunnel flag, so its reads went browser-direct. BingX klines refuse our browser origin
(403), so tunnel-only BingX charts failed; MEXC read from the user's own IP.

**Callers.** None. A plain direct account (neither flag) still reads direct; keyed
and full-account engines keep their route; the signed-packet guard is unchanged.

**Obligation.** None. **Deletion prerequisites.** None added.
