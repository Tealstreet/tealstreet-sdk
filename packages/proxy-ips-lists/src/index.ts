// The Vultr Tokyo proxy fleet: one reserved IP per proxy in
// infra/tofu/proxy-fleet/proxies.tf (apps/web's newEgressIps.test.ts holds the two
// equal). Since docs/plans/AWS_EXIT.md step 3e the master build sends every private
// path through it, so this is the only list master asks users to whitelist.
export const privateProxyIpsVultrTokyo = [
  '167.179.112.217',
  '45.63.121.235',
  '45.77.135.138',
  '108.61.162.122',
  '149.28.30.214',
  '108.160.129.108',
  '45.77.27.69',
  '45.76.52.241',
  '108.61.247.61',
];

// The AWS pools below are what legacy-master users whitelist until AWS_EXIT.md 3f.
export const privateProxyIpsSingapore = [
  '13.215.143.179',
  '54.169.98.78',
  '18.139.171.87',
  '52.76.105.51',
  '13.251.151.220',
  '54.179.219.84',
  '18.139.81.245',
];

export const privateProxyIpsTokyo = ['18.178.14.27', '35.75.147.201', '35.78.132.227', '54.238.24.246', '54.65.38.58'];

export const privateProxyIpsTokyoSmallPP = ['18.178.14.27', '35.75.147.201', '54.238.24.246'];

// Tokyo K8s cluster NAT gateway - used for WS tunnel outbound connections
export const tokyoK8sNatGatewayIp = '18.176.98.246';

export const wsProxyIps = ['54.255.57.166'];

// Exchanges whose private socket still leaves through the AWS ws-proxy (`wsProxyIps`).
// None: okx, okxeu, bitget and blofin moved to the tunnel relay in AWS_EXIT.md 3e,
// coinbase once the relay took root-path entries (5a). The hosted exchange server
// still pins the ws-proxy until its own repoint.
export const exchangesRequireWsIp: string[] = [];
