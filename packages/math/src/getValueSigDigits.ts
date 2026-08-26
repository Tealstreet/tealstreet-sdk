export const getValueSigDigits = (v: string | number) => {
  const absVal = Math.abs(typeof v === 'number' ? v : parseFloat(v));
  if (absVal === 0) {
    return 1;
  } else if (absVal >= 1000) {
    return 2;
  } else if (absVal >= 10) {
    return 3;
  } else if (absVal >= 1) {
    return 4;
  } else if (absVal >= 0.0001) {
    return 5;
  } else if (absVal >= 0.00001) {
    return 6;
  } else if (absVal >= 0.000001) {
    return 7;
  } else if (absVal >= 0.0000001) {
    return 8;
  } else if (absVal >= 0.00000001) {
    return 9;
  } else if (absVal >= 0.000000001) {
    return 10;
  } else if (absVal >= 0.0000000001) {
    return 11;
  } else {
    return 12;
  }
};
