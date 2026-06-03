# CLAUDE.md — @tealstreet/proxy-ips-lists

Private proxy IP addresses database organized by region.

- **Key file**: `src/index.ts`
- **Exports**: `privateProxyIpsSingapore`, `privateProxyIpsFrankfurt`, `privateProxyIpsTokyo`, `tokyoK8sNatGatewayIp`, `wsProxyIps`, `exchangesRequireWsIp`
- **Build**: `yarn build-force` (tsup)
- **Note**: Data-only package. Lists exchanges that require WebSocket proxy.
  `tokyoK8sNatGatewayIp` is not part of `privateProxyIpsTokyo`; it is the
  Tokyo K8s NAT gateway used by WS tunnel outbound connections. Web account
  setup code must include it separately for exchange/region flows whose user
  API-key allowlist must cover the Tokyo tunnel egress, such as BitMEX JA.
  **Gotcha:** the inclusion gate (`getWhitelistedIpsForExchange` in
  `apps/web/src/components/newApiFlow/useAccountForm.ts`) must key off the
  *resolved/default* region (first configured key — JA for BitMEX), **not** a
  strict `proxyRegion === 'JA'`. New accounts default to the first region but the
  form's region field reads back `undefined` on first render, so a strict equality
  silently drops `tokyoK8sNatGatewayIp` from the suggested whitelist until the
  user touches the selector.
