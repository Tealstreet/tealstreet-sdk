//! V8 TimSort comparison scheduling for native JavaScript comparator ports.
//! Adapted from V8 13.6.233.17 array-sort.tq; see THIRD_PARTY_NOTICES.md.
use std::cmp::Ordering;

pub fn stable_sort<T: Clone, E>(
    values: &mut [T],
    compare: impl FnMut(&T, &T) -> Result<Ordering, E>,
) -> Result<(), E> {
    if values.len() < 2 {
        return Ok(());
    }
    let mut sort = TimSort {
        work: values.to_vec(),
        compare,
        runs: Vec::new(),
        min_gallop: 7,
    };
    let mut n = values.len();
    let mut remainder = 0;
    while n >= 64 {
        remainder |= n & 1;
        n >>= 1;
    }
    let min_run = n + remainder;
    let mut low = 0;
    while low < values.len() {
        let mut len = sort.count_run(low)?;
        if len < min_run {
            let forced = min_run.min(values.len() - low);
            sort.binary_insertion(low, low + len, low + forced)?;
            len = forced;
        }
        sort.runs.push((low, len));
        sort.collapse()?;
        low += len;
    }
    while sort.runs.len() > 1 {
        let mut at = sort.runs.len() - 2;
        if at > 0 && sort.runs[at - 1].1 < sort.runs[at + 1].1 {
            at -= 1;
        }
        sort.merge_at(at)?;
    }
    values.clone_from_slice(&sort.work);
    Ok(())
}

struct TimSort<T, F> {
    work: Vec<T>,
    compare: F,
    runs: Vec<(usize, usize)>,
    min_gallop: usize,
}

impl<T: Clone, E, F: FnMut(&T, &T) -> Result<Ordering, E>> TimSort<T, F> {
    fn count_run(&mut self, low: usize) -> Result<usize, E> {
        if low + 1 == self.work.len() {
            return Ok(1);
        }
        let descending = (self.compare)(&self.work[low + 1], &self.work[low])? == Ordering::Less;
        let mut len = 2;
        while low + len < self.work.len() {
            let order = (self.compare)(&self.work[low + len], &self.work[low + len - 1])?;
            if (order == Ordering::Less) != descending {
                break;
            }
            len += 1;
        }
        if descending {
            self.work[low..low + len].reverse();
        }
        Ok(len)
    }

    fn binary_insertion(&mut self, low: usize, start: usize, high: usize) -> Result<(), E> {
        for at in start.max(low + 1)..high {
            let pivot = self.work[at].clone();
            let (mut left, mut right) = (low, at);
            while left < right {
                let mid = left + (right - left) / 2;
                if (self.compare)(&pivot, &self.work[mid])? == Ordering::Less {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            for dst in (left + 1..=at).rev() {
                self.work[dst] = self.work[dst - 1].clone();
            }
            self.work[left] = pivot;
        }
        Ok(())
    }

    fn invariant(&self, at: usize) -> bool {
        at < 2 || self.runs[at - 2].1 > self.runs[at - 1].1 + self.runs[at].1
    }

    fn collapse(&mut self) -> Result<(), E> {
        while self.runs.len() > 1 {
            let mut at = self.runs.len() - 2;
            if !self.invariant(at + 1) || !self.invariant(at) {
                if self.runs[at - 1].1 < self.runs[at + 1].1 {
                    at -= 1;
                }
                self.merge_at(at)?;
            } else if self.runs[at].1 <= self.runs[at + 1].1 {
                self.merge_at(at)?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn merge_at(&mut self, at: usize) -> Result<(), E> {
        let (mut base_a, mut len_a) = self.runs[at];
        let (base_b, mut len_b) = self.runs[at + 1];
        self.runs[at].1 += len_b;
        self.runs.remove(at + 1);
        let skip = gallop(
            &self.work,
            &self.work[base_b],
            base_a,
            len_a,
            0,
            true,
            &mut self.compare,
        )?;
        base_a += skip;
        len_a -= skip;
        if len_a == 0 {
            return Ok(());
        }
        len_b = gallop(
            &self.work,
            &self.work[base_a + len_a - 1],
            base_b,
            len_b,
            len_b - 1,
            false,
            &mut self.compare,
        )?;
        if len_b == 0 {
            return Ok(());
        }
        if len_a <= len_b {
            self.merge_low(base_a, len_a, base_b, len_b)
        } else {
            self.merge_high(base_a, len_a, base_b, len_b)
        }
    }

    fn copy_within(&mut self, from: usize, to: usize, len: usize) {
        if to < from {
            for offset in 0..len {
                self.work[to + offset] = self.work[from + offset].clone();
            }
        } else {
            for offset in (0..len).rev() {
                self.work[to + offset] = self.work[from + offset].clone();
            }
        }
    }

    fn merge_low(
        &mut self,
        base_a: usize,
        mut len_a: usize,
        base_b: usize,
        mut len_b: usize,
    ) -> Result<(), E> {
        let temp = self.work[base_a..base_a + len_a].to_vec();
        let (mut dst, mut a, mut b) = (base_a, 0, base_b);
        self.work[dst] = self.work[b].clone();
        dst += 1;
        b += 1;
        len_b -= 1;
        let mut min_gallop = self.min_gallop;
        'merge: while len_b > 0 && len_a > 1 {
            let (mut wins_a, mut wins_b) = (0, 0);
            loop {
                if (self.compare)(&self.work[b], &temp[a])? == Ordering::Less {
                    self.work[dst] = self.work[b].clone();
                    dst += 1;
                    b += 1;
                    wins_b += 1;
                    len_b -= 1;
                    wins_a = 0;
                    if len_b == 0 {
                        break 'merge;
                    }
                    if wins_b >= min_gallop {
                        break;
                    }
                } else {
                    self.work[dst] = temp[a].clone();
                    dst += 1;
                    a += 1;
                    wins_a += 1;
                    len_a -= 1;
                    wins_b = 0;
                    if len_a == 1 {
                        break 'merge;
                    }
                    if wins_a >= min_gallop {
                        break;
                    }
                }
            }
            min_gallop += 1;
            let mut first = true;
            while wins_a >= 7 || wins_b >= 7 || first {
                first = false;
                min_gallop = min_gallop.saturating_sub(1).max(1);
                self.min_gallop = min_gallop;
                wins_a = gallop(&temp, &self.work[b], a, len_a, 0, true, &mut self.compare)?;
                self.work[dst..dst + wins_a].clone_from_slice(&temp[a..a + wins_a]);
                dst += wins_a;
                a += wins_a;
                len_a -= wins_a;
                if len_a <= 1 {
                    break 'merge;
                }
                self.work[dst] = self.work[b].clone();
                dst += 1;
                b += 1;
                len_b -= 1;
                if len_b == 0 {
                    break 'merge;
                }
                wins_b = gallop(&self.work, &temp[a], b, len_b, 0, false, &mut self.compare)?;
                self.copy_within(b, dst, wins_b);
                dst += wins_b;
                b += wins_b;
                len_b -= wins_b;
                if len_b == 0 {
                    break 'merge;
                }
                self.work[dst] = temp[a].clone();
                dst += 1;
                a += 1;
                len_a -= 1;
                if len_a == 1 {
                    break 'merge;
                }
            }
            min_gallop += 1;
            self.min_gallop = min_gallop;
        }
        if len_a == 1 && len_b > 0 {
            self.copy_within(b, dst, len_b);
            self.work[dst + len_b] = temp[a].clone();
        } else if len_a > 0 {
            self.work[dst..dst + len_a].clone_from_slice(&temp[a..a + len_a]);
        }
        Ok(())
    }

    fn merge_high(
        &mut self,
        base_a: usize,
        mut len_a: usize,
        base_b: usize,
        mut len_b: usize,
    ) -> Result<(), E> {
        let temp = self.work[base_b..base_b + len_b].to_vec();
        let (mut dst, mut a, mut b) = (base_b + len_b, base_a + len_a, len_b);
        dst -= 1;
        a -= 1;
        self.work[dst] = self.work[a].clone();
        len_a -= 1;
        let mut min_gallop = self.min_gallop;
        'merge: while len_a > 0 && len_b > 1 {
            let (mut wins_a, mut wins_b) = (0, 0);
            loop {
                if (self.compare)(&temp[b - 1], &self.work[a - 1])? == Ordering::Less {
                    dst -= 1;
                    a -= 1;
                    self.work[dst] = self.work[a].clone();
                    wins_a += 1;
                    len_a -= 1;
                    wins_b = 0;
                    if len_a == 0 {
                        break 'merge;
                    }
                    if wins_a >= min_gallop {
                        break;
                    }
                } else {
                    dst -= 1;
                    b -= 1;
                    self.work[dst] = temp[b].clone();
                    wins_b += 1;
                    len_b -= 1;
                    wins_a = 0;
                    if len_b == 1 {
                        break 'merge;
                    }
                    if wins_b >= min_gallop {
                        break;
                    }
                }
            }
            min_gallop += 1;
            let mut first = true;
            while wins_a >= 7 || wins_b >= 7 || first {
                first = false;
                min_gallop = min_gallop.saturating_sub(1).max(1);
                self.min_gallop = min_gallop;
                let k = gallop(
                    &self.work,
                    &temp[b - 1],
                    base_a,
                    len_a,
                    len_a - 1,
                    true,
                    &mut self.compare,
                )?;
                wins_a = len_a - k;
                dst -= wins_a;
                a -= wins_a;
                self.copy_within(a, dst, wins_a);
                len_a -= wins_a;
                if len_a == 0 {
                    break 'merge;
                }
                dst -= 1;
                b -= 1;
                self.work[dst] = temp[b].clone();
                len_b -= 1;
                if len_b == 1 {
                    break 'merge;
                }
                let k = gallop(
                    &temp,
                    &self.work[a - 1],
                    0,
                    len_b,
                    len_b - 1,
                    false,
                    &mut self.compare,
                )?;
                wins_b = len_b - k;
                dst -= wins_b;
                b -= wins_b;
                self.work[dst..dst + wins_b].clone_from_slice(&temp[b..b + wins_b]);
                len_b -= wins_b;
                if len_b <= 1 {
                    break 'merge;
                }
                dst -= 1;
                a -= 1;
                self.work[dst] = self.work[a].clone();
                len_a -= 1;
                if len_a == 0 {
                    break 'merge;
                }
            }
            min_gallop += 1;
            self.min_gallop = min_gallop;
        }
        if len_b == 1 && len_a > 0 {
            dst -= len_a;
            a -= len_a;
            self.copy_within(a, dst, len_a);
            self.work[dst - 1] = temp[b - 1].clone();
        } else if len_b > 0 {
            self.work[dst - len_b..dst].clone_from_slice(&temp[..len_b]);
        }
        Ok(())
    }
}

fn gallop<T, E, F: FnMut(&T, &T) -> Result<Ordering, E>>(
    array: &[T],
    key: &T,
    base: usize,
    len: usize,
    hint: usize,
    right: bool,
    compare: &mut F,
) -> Result<usize, E> {
    let before = |value: &T, key: &T, compare: &mut F| -> Result<bool, E> {
        if right {
            Ok(compare(key, value)? != Ordering::Less)
        } else {
            Ok(compare(value, key)? == Ordering::Less)
        }
    };
    let (mut last, mut offset) = (0, 1);
    let (mut low, mut high);
    if before(&array[base + hint], key, compare)? {
        let max = len - hint;
        while offset < max {
            if !before(&array[base + hint + offset], key, compare)? {
                break;
            }
            last = offset;
            offset = (offset * 2 + 1).min(max);
        }
        offset = offset.min(max);
        low = hint + last + 1;
        high = hint + offset;
    } else {
        let max = hint + 1;
        while offset < max {
            if before(&array[base + hint - offset], key, compare)? {
                break;
            }
            last = offset;
            offset = (offset * 2 + 1).min(max);
        }
        offset = offset.min(max);
        low = if offset > hint { 0 } else { hint - offset + 1 };
        high = hint - last;
    }
    while low < high {
        let mid = low + (high - low) / 2;
        if before(&array[base + mid], key, compare)? {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    Ok(high)
}
