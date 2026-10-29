# GRVT IP-whitelist error text

GRVT error `1008` no longer names the AWS Tokyo EKS NAT IP (`18.176.98.246`).
The Rust notice and the safe-cex TS reference now send users to the Tealstreet
IPs shown in the API key form, which follow the egress fleet.

## Obligations

- None for this text. The API key form takes its IPs from TS
  `exchangeMetadata`, which lists `privateProxyIpsVultrTokyo` since step 3e.
  GRVT `privateProxyIps` in the generated
  `sdk-rs/safe-cex-types/src/exchange_metadata.json` still includes the NAT IP;
  this change does not regenerate it.
