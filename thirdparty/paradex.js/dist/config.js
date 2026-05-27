import { ETHEREUM_MAINNET_CHAIN_ID, ETHEREUM_TESTNET_CHAIN_ID, STARKNET_MAINNET_CHAIN_ID, STARKNET_TESTNET_CHAIN_ID, } from './constants.js';
/**
 * Fetches the Paradex config for the given environment.
 * This is required for account derivation.
 */
export async function fetchConfig(environment) {
    assertParadexEnvironment(environment);
    const apiUrl = getParadexApiUrl(environment);
    const resp = await fetch(`${apiUrl}/system/config`);
    if (!resp.ok) {
        throw new Error(`Failed to fetch Paradex config: ${resp.statusText} ${resp.status}`);
    }
    const jsonResp = await resp.json();
    const config = jsonResp;
    return buildConfig(config);
}
function assertParadexEnvironment(environment) {
    if (environment !== 'testnet' && environment !== 'prod') {
        throw new Error(`Invalid Paradex environment: ${environment}`);
    }
}
function getParadexApiUrl(environment) {
    return `https://api.${environment}.paradex.trade/v1`;
}
export function buildConfig(rawConfig) {
    const bridgedTokens = Object.fromEntries(rawConfig.bridged_tokens.map((token) => [
        token.symbol,
        {
            name: token.name,
            symbol: token.symbol,
            decimals: token.decimals,
            l1TokenAddress: token.l1_token_address,
            l1BridgeAddress: token.l1_bridge_address,
            l2TokenAddress: token.l2_token_address,
            l2BridgeAddress: token.l2_bridge_address,
        },
    ]));
    return {
        paradexFullNodeRpcUrl: rawConfig.starknet_fullnode_rpc_url,
        paradexChainId: rawConfig.starknet_chain_id,
        ethereumChainId: rawConfig.l1_chain_id,
        starknetChainId: getStarknetChainId(rawConfig),
        paraclearAccountHash: rawConfig.paraclear_account_hash,
        paraclearAccountProxyHash: rawConfig.paraclear_account_proxy_hash,
        paraclearAddress: rawConfig.paraclear_address,
        paraclearDecimals: rawConfig.paraclear_decimals,
        bridgedTokens,
    };
}
function getStarknetChainId(rawConfig) {
    switch (rawConfig.l1_chain_id) {
        case ETHEREUM_MAINNET_CHAIN_ID:
            return STARKNET_MAINNET_CHAIN_ID;
        case ETHEREUM_TESTNET_CHAIN_ID:
        default:
            return STARKNET_TESTNET_CHAIN_ID;
    }
}
