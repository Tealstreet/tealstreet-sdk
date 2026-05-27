// Regression tests for the EIP-712 order signing patches:
//   - b86e97af04 fix(grvt): implement EIP-712 order signing using SDK
//   - 672a636458 feat(grvt): EIP-712 builder fee signing for order placement
//
// These tests pin deterministic signatures for fixed inputs. If an upstream
// sync (or any refactor) changes the EIP-712 type definitions, domain, or
// field ordering, these signatures will diverge and the tests fail —
// catching the regression before it reaches GRVT's API which would simply
// reject the misfiled signature.
//
// Test vectors use the canonical "all-ones" private key:
//   privateKey = 0x01..01 (32 bytes)
//   address    = 0x7e5f4552091A69125d5DfCb7b8C2659029395Bdf
import { Signer } from '../src/signing/signer';
import { Order, OrderWithBuilderFee, PrimaryType } from '../src/signing/types';

const PRIVATE_KEY = '0x' + '01'.repeat(32);

describe('Signer.sign — EIP-712 order signing (Tealstreet patches)', () => {
  it('produces a deterministic signature for a canonical Order', () => {
    const data = {
      ...Order,
      message: {
        subAccountID: '1234567890',
        isMarket: false,
        timeInForce: 1,
        postOnly: false,
        reduceOnly: false,
        legs: [
          {
            assetID: '79228162514264337593543950337',
            contractSize: '1000000000',
            limitPrice: '50000000000',
            isBuyingContract: true,
          },
        ],
        nonce: 42,
        expiration: '1700000000000000000',
      },
    };

    const sig = Signer.sign(PRIVATE_KEY, data as any);
    // Pin the full 65-byte (0x + 130 hex) signature.
    expect(sig).toBe(
      '0x0a721403575ee7bf1d3497c51d524de10a835b1f8a50c8db02093c6ee74aa5033f97459f629909a3d82e97ee0b653c08871481a71a0d8ecaf9878d2faf9e31e51c'
    );
  });

  it('produces a deterministic signature for an OrderWithBuilderFee', () => {
    const data = {
      ...OrderWithBuilderFee,
      message: {
        subAccountID: '1234567890',
        isMarket: false,
        timeInForce: 1,
        postOnly: false,
        reduceOnly: false,
        legs: [
          {
            assetID: '79228162514264337593543950337',
            contractSize: '1000000000',
            limitPrice: '50000000000',
            isBuyingContract: true,
          },
        ],
        builder: '0x000000000000000000000000000000000000beef',
        builderFee: 100,
        nonce: 42,
        expiration: '1700000000000000000',
      },
    };

    const sig = Signer.sign(PRIVATE_KEY, data as any);
    expect(sig).toBe(
      '0x41162e249c152d579c97624668b29ae2280bb0309134436a39becef529f2a0ae211a649c18ef9d419df6a32a93f2d1852f81f60798150bc818fc39527586e1c21b'
    );
  });

  it('Signer.decode splits a signature into r/s/v components', () => {
    // A sample 65-byte sig: r=0x11..11, s=0x22..22, v=27 (0x1b)
    const sig = '0x' + '11'.repeat(32) + '22'.repeat(32) + '1b';
    const decoded = Signer.decode(sig);
    expect(decoded.r).toBe('0x' + '11'.repeat(32));
    expect(decoded.s).toBe('0x' + '22'.repeat(32));
    expect(decoded.v).toBe(27);
  });
});

describe('PrimaryType (Tealstreet patch: OrderWithBuilderFee enum)', () => {
  it('Order and OrderWithBuilderFee primary types are distinct string keys', () => {
    expect(PrimaryType.Order).toBe('Order');
    expect(PrimaryType.OrderWithBuilderFee).toBe('OrderWithBuilderFee');
  });
});
