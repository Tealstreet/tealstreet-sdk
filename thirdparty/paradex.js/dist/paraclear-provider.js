import * as Starknet from 'starknet';
export class DefaultProvider extends Starknet.RpcProvider {
    constructor(config) {
        super({
            nodeUrl: config.paradexFullNodeRpcUrl,
            chainId: Starknet.shortString.encodeShortString(config.paradexChainId),
        });
    }
}
