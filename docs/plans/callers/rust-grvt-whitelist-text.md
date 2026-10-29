# GRVT IP-whitelist error text

GRVT error `1008` no longer names the AWS Tokyo EKS NAT IP (`18.176.98.246`).
The Rust notice and the safe-cex TS reference now send users to the Tealstreet
IPs shown in the API key form, which follow the egress fleet.

## Obligations

- NX: the API key form must list the Vultr Tokyo private proxy IPs for GRVT
  once egress 3e moves GRVT private traffic. GRVT `privateProxyIps` in
  `sdk-rs/safe-cex-types/src/exchange_metadata.json` still includes the NAT IP;
  this change does not edit that metadata.
