import { BigNumber } from 'bignumber.js';
import * as Starknet from 'starknet';
/**
 * Get the Paraclear balance of the given token for the given account.
 */
export async function getTokenBalance(params) {
    const token = params.config.bridgedTokens[params.token];
    if (token == null) {
        throw new Error(`Token ${params.token} is not supported`);
    }
    const result = await params.provider.callContract({
        contractAddress: params.config.paraclearAddress,
        entrypoint: 'getTokenAssetBalance',
        calldata: Starknet.CallData.compile([
            params.account.address,
            token.l2TokenAddress,
        ]),
    }, 'latest');
    const value = result?.[0];
    if (value == null) {
        throw new Error('Failed to get token balance');
    }
    const valueBn = new BigNumber(value);
    if (valueBn.isNaN()) {
        throw new Error('Failed to parse token balance');
    }
    const chainSizeBn = valueBn;
    const sizeBn = fromChainSize(chainSizeBn, params.config.paraclearDecimals);
    return { size: sizeBn.toString() };
}
/**
 * Socialized losses happen when Paradex Insurance Fund is bankrupt
 * due to large amounts of unprofitable liquidations. When socialized
 * losses are active, (socialized loss factor > 0), the amount that
 * the user will receive when withdrawing will be smaller than the
 * requested amount.
 */
export async function getSocializedLossFactor(params) {
    const result = await params.provider.callContract({
        contractAddress: params.config.paraclearAddress,
        entrypoint: 'getSocializedLossFactor',
    });
    const value = result?.[0];
    if (value == null) {
        throw new Error('Failed to get socialized loss factor');
    }
    const valueBn = new BigNumber(value);
    if (valueBn.isNaN()) {
        throw new Error('Failed to parse socialized loss factor');
    }
    const chainFactorBn = valueBn;
    const factorBn = fromChainSize(chainFactorBn, params.config.paraclearDecimals);
    return { socializedLossFactor: factorBn.toString() };
}
/**
 * The receivable amount is calculated based on the current
 * socialized loss factor: amount * (1 - socializedLossFactor)
 *
 * If the socialized loss factor is 0, the receivable amount
 * will be equal to the requested amount.
 */
export async function getReceivableAmount(params) {
    const amountBn = new BigNumber(params.amount);
    if (amountBn.isNaN()) {
        throw new Error('Invalid amount');
    }
    const token = params.config.bridgedTokens[params.token];
    if (token == null) {
        throw new Error(`Token ${params.token} is not supported`);
    }
    const { socializedLossFactor } = await getSocializedLossFactor({
        config: params.config,
        provider: params.provider,
    });
    const receivableAmount = amountBn.times(BigNumber(1).minus(socializedLossFactor));
    const receivableAmountChainBn = toChainSize(receivableAmount.toString(), token.decimals);
    return {
        receivableAmount: receivableAmount.toString(),
        receivableAmountChain: receivableAmountChainBn.toString(),
        socializedLossFactor,
    };
}
function getResourceBounds() {
    // ensure unique txn hash on subsequent calls via `intNoise`
    const maxL1Gas = BigNumber('0x989680').plus(intNoise(10000)); // 10M gas units
    const maxL1DAGas = BigNumber('0x989680').plus(intNoise(10000)); // 10M gas units
    const maxL2Gas = BigNumber('0x5f5e100').plus(intNoise(10000)); // 100M gas units
    const maxL1GasPrice = '0x2540be400'; // 10B WEI per gas unit
    const maxL1DAGasPrice = '0x2540be400'; // 10B WEI per gas unit
    const maxL2GasPrice = '0x2540be400'; // 10B WEI per gas unit
    const maxL1GasHex = `0x${maxL1Gas.toString(16)}`;
    const maxL1DAGasHex = `0x${maxL1DAGas.toString(16)}`;
    const maxL2GasHex = `0x${maxL2Gas.toString(16)}`;
    return {
        l1_gas: {
            max_amount: maxL1GasHex,
            max_price_per_unit: maxL1GasPrice,
        },
        l1_data_gas: {
            max_amount: maxL1DAGasHex,
            max_price_per_unit: maxL1DAGasPrice,
        },
        l2_gas: {
            max_amount: maxL2GasHex,
            max_price_per_unit: maxL2GasPrice,
        },
    };
}
/**
 * Withdraw funds from Paraclear for the given account.
 *
 * Automatically make a batch transaction with `initiate_withdrawal`
 * call to the Paraclear contract along with the transaction passed
 * as `params.bridgeCall` The batch call is atomic. If either of
 * the transactions fail, the entire batch gets reverted.
 *
 * If socialized loss is active, the bridge call must be constructed
 * with an amount that accounts for the loss. To calculate that amount,
 * use {@link getReceivableAmount}. Failing to do so can result in a
 * failed withdrawal.
 */
export async function withdraw(params) {
    const token = params.config.bridgedTokens[params.token];
    if (token == null) {
        throw new Error(`Token ${params.token} is not supported`);
    }
    const chainAmountBn = toChainSize(params.amount, params.config.paraclearDecimals);
    const resourceBounds = getResourceBounds();
    const result = await params.account.execute([
        {
            contractAddress: params.config.paraclearAddress,
            entrypoint: 'withdraw',
            calldata: [token.l2TokenAddress, chainAmountBn.toString()],
        },
        ...(Array.isArray(params.bridgeCall)
            ? params.bridgeCall
            : [params.bridgeCall]),
    ], { resourceBounds });
    return { hash: result.transaction_hash };
}
function fromChainSize(size, decimals) {
    return new BigNumber(size).div(10 ** decimals);
}
function toChainSize(size, decimals) {
    return new BigNumber(size)
        .times(10 ** decimals)
        .integerValue(BigNumber.ROUND_FLOOR);
}
/**
 * Generates a pseudorandom integer between 0 and `max`.
 * @param max Maximum value for the noise.
 */
function intNoise(max) {
    return Math.round(Math.random() * max);
}
