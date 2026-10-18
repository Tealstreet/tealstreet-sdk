//! Stable, fallible sorting without a total-order requirement on JS callbacks.
use super::value::Thrown;
use std::cmp::Ordering;

pub(crate) fn stable_sort<T: Clone>(
    values: &mut [T],
    mut compare: impl FnMut(&T, &T) -> Result<Ordering, Thrown>,
) -> Result<(), Thrown> {
    let len = values.len();
    if len < 2 {
        return Ok(());
    }
    let mut source = values.to_vec();
    let mut target = source.clone();
    let mut width = 1;
    while width < len {
        for start in (0..len).step_by(width * 2) {
            let middle = (start + width).min(len);
            let end = (middle + width).min(len);
            let (mut left, mut right) = (start, middle);
            for slot in &mut target[start..end] {
                if left < middle
                    && (right == end
                        || compare(&source[left], &source[right])? != Ordering::Greater)
                {
                    *slot = source[left].clone();
                    left += 1;
                } else {
                    *slot = source[right].clone();
                    right += 1;
                }
            }
        }
        std::mem::swap(&mut source, &mut target);
        width *= 2;
    }
    values.clone_from_slice(&source);
    Ok(())
}
