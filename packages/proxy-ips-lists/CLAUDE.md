# CLAUDE.md — @tealstreet/proxy-ips-lists

Private proxy IP addresses database organized by region.

- **Key file**: `src/index.ts`
- **Exports**: `privateProxyIpsVultrTokyo`, `privateProxyIpsSingapore`, `privateProxyIpsTokyo`, `tokyoK8sNatGatewayIp`, `wsProxyIps`, `exchangesRequireWsIp`
- **Routing metadata stays all-Vultr.** Every whitelist in master's
  `exchangeMetadata` is `privateProxyIpsVultrTokyo`; its invariant test stays
  unchanged. AWS exports remain active for rollback and legacy whitelabels.
- **Display policy** lives in `src/whitelist.ts`, shared by web and docs. Its
  `legacyWhitelist.ts` maps venue/region to the existing AWS exports by symbol,
  projected from the cited legacy-master revision, without changing routing.
  Fleet comes first, followed by legacy AWS and WS/Bitunix extras. Copy uses the
  unique union unless a verified venue cap requires fleet only; legacy addresses
  remain visible and optional. Unknown caps are not guessed. The legacy WS gate
  is retained separately because master's `exchangesRequireWsIp` is now empty.
- **Build**: `yarn build-force` (tsup)
- **Note**: Data-only package. Lists exchanges that require WebSocket proxy.
  `tokyoK8sNatGatewayIp` is not part of `privateProxyIpsTokyo`; it is the
  Tokyo K8s NAT gateway used by WS tunnel outbound connections (legacy only;
  master's GRVT socket leaves through the Vultr fleet). Web account
  setup code must include it separately for exchange/region flows whose user
  API-key allowlist must cover the Tokyo tunnel egress.
  The display helper resolves a configured default region when the form has
  not selected one yet, so the original region's addresses are not dropped.
- The former Frankfurt private proxy pool was retired in July 2026. Do not add
  its released addresses back to user-facing allowlist suggestions.
