import { convertPxStrToInt } from './convertPxStrToInt';

export const convertDimToInt = (dim: string | number | unknown) => {
  if (typeof dim === 'number') {
    return dim;
  } else if (typeof dim === 'string') {
    if (dim.endsWith('px')) {
      return convertPxStrToInt(dim);
    } else {
      return parseInt(dim);
    }
  } else {
    return null;
  }
};
