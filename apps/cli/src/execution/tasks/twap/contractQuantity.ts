import type { ExactQuantity } from '../../confirmed/types';

export const baseQuantityToContracts = (baseQuantity: ExactQuantity, contractSize: ExactQuantity): ExactQuantity => {
  const baseAtoms = exactAtoms(baseQuantity, 'base quantity');
  const contractSizeAtoms = exactAtoms(contractSize, 'contract size');
  if (contractSizeAtoms <= 0n) throw new Error('Contract size must be positive');

  let numerator = baseAtoms * 10n ** BigInt(contractSize.atomScale);
  let denominator = contractSizeAtoms * 10n ** BigInt(baseQuantity.atomScale);
  const divisor = greatestCommonDivisor(numerator, denominator);
  numerator /= divisor;
  denominator /= divisor;

  let factorsOfTwo = 0;
  while (denominator % 2n === 0n) {
    denominator /= 2n;
    factorsOfTwo += 1;
  }
  let factorsOfFive = 0;
  while (denominator % 5n === 0n) {
    denominator /= 5n;
    factorsOfFive += 1;
  }
  if (denominator !== 1n) {
    throw new Error('Base quantity does not resolve to an exact decimal number of contracts');
  }

  const atomScale = Math.max(factorsOfTwo, factorsOfFive);
  const atoms =
    numerator * 2n ** BigInt(atomScale - factorsOfTwo) * 5n ** BigInt(atomScale - factorsOfFive);
  return exactQuantity(atoms, atomScale);
};

export const contractsToBaseQuantity = (
  contractQuantity: ExactQuantity,
  contractSize: ExactQuantity,
): ExactQuantity => {
  const quantityAtoms = exactAtoms(contractQuantity, 'contract quantity');
  const contractSizeAtoms = exactAtoms(contractSize, 'contract size');
  return exactQuantity(
    quantityAtoms * contractSizeAtoms,
    contractQuantity.atomScale + contractSize.atomScale,
  );
};

const exactAtoms = (quantity: ExactQuantity, name: string): bigint => {
  if (!/^\d+$/.test(quantity.atoms) || !Number.isSafeInteger(quantity.atomScale) || quantity.atomScale < 0) {
    throw new Error(`${name} must be a non-negative exact quantity`);
  }
  return BigInt(quantity.atoms);
};

const greatestCommonDivisor = (left: bigint, right: bigint): bigint => {
  while (right !== 0n) {
    const remainder = left % right;
    left = right;
    right = remainder;
  }
  return left;
};

const exactQuantity = (initialAtoms: bigint, initialScale: number): ExactQuantity => {
  let atoms = initialAtoms;
  let atomScale = initialScale;
  while (atomScale > 0 && atoms % 10n === 0n) {
    atoms /= 10n;
    atomScale -= 1;
  }
  const digits = atoms.toString();
  if (atomScale === 0) return { atoms: digits, decimal: digits, atomScale };
  const padded = digits.padStart(atomScale + 1, '0');
  const split = padded.length - atomScale;
  return { atoms: digits, decimal: `${padded.slice(0, split)}.${padded.slice(split)}`, atomScale };
};

export const exactFromFiniteNumber = (value: number): ExactQuantity | undefined => {
  if (!Number.isFinite(value) || value <= 0) return undefined;
  const decimal = value.toString();
  if (decimal.includes('e') || decimal.includes('E')) {
    return undefined;
  }
  const [whole, frac = ''] = decimal.split('.');
  const atoms = `${whole}${frac}`.replace(/^0+(?=\d)/, '');
  return { decimal, atoms, atomScale: frac.length };
};
