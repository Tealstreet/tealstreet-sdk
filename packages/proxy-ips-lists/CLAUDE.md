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
