# CLAUDE.md — @tealstreet/proxy-ips-lists

Private proxy IP addresses database organized by region.

- **Key file**: `src/index.ts`
- **Exports**: `privateProxyIpsVultrTokyo`, `privateProxyIpsSingapore`, `privateProxyIpsTokyo`, `tokyoK8sNatGatewayIp`, `wsProxyIps`, `exchangesRequireWsIp`
- **Routing and user-facing whitelist copy stay all-Vultr.** Every whitelist in
  master's `exchangeMetadata` is `privateProxyIpsVultrTokyo`, and
  `whitelist.ts` returns that same fleet for display and copy. The old AWS
  exports are retained only as historical named data while retired references
  are cleaned up; do not add them back to account setup copy.
- **Display policy** lives in `src/whitelist.ts`, shared by web and docs. It
  intentionally ignores `legacyWhitelist.ts` so new account setup, refused-key
  recovery and docs all copy only the active fleet addresses. Keep old AWS
  addresses out of the copy surface.
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
