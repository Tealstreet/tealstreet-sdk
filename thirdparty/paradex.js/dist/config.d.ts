import type { Hex } from './types.js';
interface RawBridgedTokenConfig {
    readonly name: string;
    readonly symbol: string;
    readonly decimals: number;
    readonly l1_token_address: Hex;
    readonly l1_bridge_address: Hex;
    readonly l2_token_address: Hex;
    readonly l2_bridge_address: Hex;
}
export interface RawParadexConfig {
    readonly starknet_gateway_url: string;
    readonly starknet_fullnode_rpc_url: string;
    readonly starknet_chain_id: string;
    readonly block_explorer_url: string;
    readonly paraclear_address: Hex;
    readonly paraclear_decimals: number;
    readonly paraclear_account_proxy_hash: Hex;
    readonly paraclear_account_hash: Hex;
    readonly bridged_tokens: readonly RawBridgedTokenConfig[];
    readonly l1_core_contract_address: Hex;
    readonly l1_operator_address: Hex;
    readonly l1_chain_id: string;
}
interface BridgedTokenConfig {
    readonly name: string;
    readonly symbol: string;
    readonly decimals: number;
    readonly l1TokenAddress: Hex;
    readonly l1BridgeAddress: Hex;
    readonly l2TokenAddress: Hex;
    readonly l2BridgeAddress: Hex;
}
export interface ParadexConfig {
    readonly paradexFullNodeRpcUrl: string;
    readonly paradexChainId: string;
    readonly ethereumChainId: string;
    /** Derived from `ethereumChainId` */
    readonly starknetChainId: string;
    readonly paraclearAccountHash: Hex;
    readonly paraclearAccountProxyHash: Hex;
    readonly paraclearAddress: Hex;
    readonly paraclearDecimals: number;
    readonly bridgedTokens: Record<string, BridgedTokenConfig>;
}
type ParadexEnvironment = 'testnet' | 'prod';
/**
 * Fetches the Paradex config for the given environment.
 * This is required for account derivation.
 */
export declare function fetchConfig(environment: ParadexEnvironment): Promise<ParadexConfig>;
export declare function buildConfig(rawConfig: RawParadexConfig): ParadexConfig;
export {};
