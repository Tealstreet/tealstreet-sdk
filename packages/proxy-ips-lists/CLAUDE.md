# CLAUDE.md — @tealstreet/proxy-ips-lists

Private proxy IP addresses database organized by region.

- **Key file**: `src/index.ts`
- **Exports**: `privateProxyIpsVultrTokyo`, `privateProxyIpsSingapore`, `privateProxyIpsTokyo`, `tokyoK8sNatGatewayIp`, `wsProxyIps`, `exchangesRequireWsIp`
- **Master shows only `privateProxyIpsVultrTokyo`** (the nine reserved IPs in
  `infra/tofu/proxy-fleet/proxies.tf`; `apps/web`'s `newEgressIps.test.ts` holds
  them equal, since this package's mirror has no `infra/`). Every whitelist in
  `exchangeMetadata` is that list since `docs/plans/AWS_EXIT.md` step 3e, and
  `exchangeMetadata.test.ts` fails if an AWS pool comes back. The AWS lists stay
  for legacy-master and go with step 3f. `exchangesRequireWsIp` is coinbase only:
  okx, okxeu, bitget and blofin moved to the relay.
- **Build**: `yarn build-force` (tsup)
- **Note**: Data-only package. Lists exchanges that require WebSocket proxy.
  `tokyoK8sNatGatewayIp` is not part of `privateProxyIpsTokyo`; it is the
  Tokyo K8s NAT gateway used by WS tunnel outbound connections (legacy only;
  master's GRVT socket leaves through the Vultr fleet). Web account
  setup code must include it separately for exchange/region flows whose user
  API-key allowlist must cover the Tokyo tunnel egress (originally BitMEX JA; BitMEX retired 2026-10).
  **Gotcha:** the inclusion gate (`getWhitelistedIpsForExchange` in
  `apps/web/src/components/newApiFlow/useAccountForm.ts`) must key off the
  *resolved/default* region (first configured key), **not** a
  strict `proxyRegion === 'JA'`. New accounts default to the first region but the
  form's region field reads back `undefined` on first render, so a strict equality
  silently drops `tokyoK8sNatGatewayIp` from the suggested whitelist until the
  user touches the selector.
- The former Frankfurt private proxy pool was retired in July 2026. Do not add
  its released addresses back to user-facing allowlist suggestions.
