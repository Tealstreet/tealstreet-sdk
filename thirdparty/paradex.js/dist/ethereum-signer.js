export function ethersSignerAdapter(ethersSigner) {
    return {
        async signTypedData(typedData) {
            return await ethersSigner.signTypedData(typedData.domain, typedData.types, typedData.message);
        },
    };
}
/**
 * Returns the typed data that needs to be signed by an Ethereum
 * wallet in order to generate a Paradex account.
 * @returns The typed data object.
 */
export function buildEthereumStarkKeyTypedData(ethereumChainId) {
    return {
        domain: {
            name: 'Paradex',
            chainId: ethereumChainId,
            version: '1',
        },
        primaryType: 'Constant',
        types: {
            Constant: [{ name: 'action', type: 'string' }],
        },
        message: {
            action: 'STARK Key',
        },
    };
}
