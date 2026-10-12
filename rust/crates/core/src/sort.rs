//! `Array.prototype.sort` with a comparator, as V8 runs it: the TimSort in
//! `third_party/v8/builtins/array-sort.tq` that Node 24.21.0 compiles, ported step for step.
//!
//! Any stable sort gives the same order when the comparator is consistent. Many of the game's
//! comparators are not: `shared/formation.js` rounds a projected difference to centimetres, so two
//! units a few millimetres apart compare as equal while a third may not. The result then depends on
//! which pairs the sort happens to compare, so this port calls the comparator on the same pairs, in
//! the same order, as V8 does.
//!
//! An array shorter than 64 is one run: the longest ascending (or strictly descending, then reversed)
//! prefix, then binary insertion of the rest. Longer arrays add the run stack, merges and galloping.
//! V8's checks for Smi overflow (offsets past 2^30) are left out: no array here comes near that size.

/// V8's `kMinGallopWins`: a merge starts galloping after one side wins this many times in a row.
const MIN_GALLOP_WINS: isize = 7;

/// Sorts `v` as `v.sort(compare)` does in V8. `compare` returns the JavaScript comparator's number;
/// NaN counts as 0, as `SortCompareUserFn` makes it.
pub fn sort_by<T: Copy>(v: &mut [T], compare: impl FnMut(T, T) -> f64) {
    let length = v.len() as isize;
    if length < 2 {
        return;
    }
    let mut sort = TimSort {
        work: v,
        temp: Vec::new(),
        runs: Vec::new(),
        min_gallop: MIN_GALLOP_WINS,
        compare,
    };
    // `ArrayTimSortImpl`: find natural runs left to right, extend short ones to the minimum run
    // length by binary insertion, and merge as the run stack requires.
    let mut remaining = length;
    let mut low = 0;
    let min_run = min_run_length(remaining);
    while remaining != 0 {
        let mut run = sort.count_and_make_run(low, low + remaining);
        if run < min_run {
            let forced = min_run.min(remaining);
            sort.binary_insertion_sort(low, low + run, low + forced);
            run = forced;
        }
        sort.runs.push(Run {
            base: low,
            length: run,
        });
        sort.merge_collapse();
        low += run;
        remaining -= run;
    }
    sort.merge_force_collapse();
}

/// `ComputeMinRunLength`: n itself below 64, otherwise between 32 and 64.
fn min_run_length(mut n: isize) -> isize {
    let mut r = 0;
    while n >= 64 {
        r |= n & 1;
        n >>= 1;
    }
    n + r
}

#[derive(Clone, Copy)]
struct Run {
    base: isize,
    length: isize,
}

/// How a merge ends: `Succeed` or `CopyB` in `MergeLow`, `Succeed` or `CopyA` in `MergeHigh`.
enum End {
    Succeed,
    CopyOther,
}

/// V8's `SortState`, less what only the JavaScript array needs.
struct TimSort<'a, T, F> {
    work: &'a mut [T],
    temp: Vec<T>,
    runs: Vec<Run>,
    min_gallop: isize,
    compare: F,
}

/// `SortCompareUserFn`: the comparator's number, with NaN read as 0.
fn order<T, F: FnMut(T, T) -> f64>(compare: &mut F, a: T, b: T) -> f64 {
    let v = compare(a, b);
    if v.is_nan() { 0.0 } else { v }
}

/// `GallopLeft`: where `key` belongs in the sorted `array[base..base + length]`, left of any equal
/// elements, searching outward from `hint`.
fn gallop_left<T: Copy, F: FnMut(T, T) -> f64>(
    compare: &mut F,
    array: &[T],
    key: T,
    base: isize,
    length: isize,
    hint: isize,
) -> isize {
    let at = |i: isize| array[i as usize];
    let mut last_ofs = 0;
    let mut offset = 1;
    if order(compare, at(base + hint), key) < 0.0 {
        // a[base + hint] < key: gallop right.
        let max_ofs = length - hint;
        while offset < max_ofs {
            if order(compare, at(base + hint + offset), key) >= 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_ofs);
        last_ofs += hint;
        offset += hint;
    } else {
        // key <= a[base + hint]: gallop left.
        let max_ofs = hint + 1;
        while offset < max_ofs {
            if order(compare, at(base + hint - offset), key) < 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_ofs);
        let tmp = last_ofs;
        last_ofs = hint - offset;
        offset = hint - tmp;
    }
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        if order(compare, at(base + m), key) < 0.0 {
            last_ofs = m + 1;
        } else {
            offset = m;
        }
    }
    offset
}

/// `GallopRight`: like `gallop_left`, but right of any equal elements.
fn gallop_right<T: Copy, F: FnMut(T, T) -> f64>(
    compare: &mut F,
    array: &[T],
    key: T,
    base: isize,
    length: isize,
    hint: isize,
) -> isize {
    let at = |i: isize| array[i as usize];
    let mut last_ofs = 0;
    let mut offset = 1;
    if order(compare, key, at(base + hint)) < 0.0 {
        // key < a[base + hint]: gallop left.
        let max_ofs = hint + 1;
        while offset < max_ofs {
            if order(compare, key, at(base + hint - offset)) >= 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_ofs);
        let tmp = last_ofs;
        last_ofs = hint - offset;
        offset = hint - tmp;
    } else {
        // a[base + hint] <= key: gallop right.
        let max_ofs = length - hint;
        while offset < max_ofs {
            if order(compare, key, at(base + hint + offset)) < 0.0 {
                break;
            }
            last_ofs = offset;
            offset = (offset << 1) + 1;
        }
        offset = offset.min(max_ofs);
        last_ofs += hint;
        offset += hint;
    }
    last_ofs += 1;
    while last_ofs < offset {
        let m = last_ofs + ((offset - last_ofs) >> 1);
        if order(compare, key, at(base + m)) < 0.0 {
            offset = m;
        } else {
            last_ofs = m + 1;
        }
    }
    offset
}

/// Copies `length` elements; V8's `Copy` between two arrays.
fn copy<T: Copy>(from: &[T], from_pos: isize, to: &mut [T], to_pos: isize, length: isize) {
    let (from_pos, to_pos, length) = (from_pos as usize, to_pos as usize, length as usize);
    to[to_pos..to_pos + length].copy_from_slice(&from[from_pos..from_pos + length]);
}

/// V8's `Copy` within one array, which copies from the back when the ranges overlap that way.
fn shift<T: Copy>(array: &mut [T], from_pos: isize, to_pos: isize, length: isize) {
    let (from_pos, to_pos, length) = (from_pos as usize, to_pos as usize, length as usize);
    array.copy_within(from_pos..from_pos + length, to_pos);
}

impl<T: Copy, F: FnMut(T, T) -> f64> TimSort<'_, T, F> {
    fn at(&self, i: isize) -> T {
        self.work[i as usize]
    }

    fn set(&mut self, i: isize, value: T) {
        self.work[i as usize] = value;
    }

    /// `BinaryInsertionSort`: [low, start) is sorted; insert each of [start, high) after the last
    /// element it does not sort before.
    fn binary_insertion_sort(&mut self, low: isize, start: isize, high: isize) {
        let mut start = if low == start { start + 1 } else { start };
        while start < high {
            let mut left = low;
            let mut right = start;
            let pivot = self.at(right);
            while left < right {
                let mid = left + ((right - left) >> 1);
                let element_mid = self.at(mid);
                if order(&mut self.compare, pivot, element_mid) < 0.0 {
                    right = mid;
                } else {
                    left = mid + 1;
                }
            }
            shift(self.work, left, left + 1, start - left);
            self.set(left, pivot);
            start += 1;
        }
    }

    /// `CountAndMakeRun`: the length of the run starting at `low`, ascending (`a[i + 1]` not below
    /// `a[i]`) or strictly descending, which is reversed in place.
    fn count_and_make_run(&mut self, low_arg: isize, high: isize) -> isize {
        let low = low_arg + 1;
        if low == high {
            return 1;
        }
        let mut run = 2;
        let element_low = self.at(low);
        let element_low_pred = self.at(low - 1);
        let descending = order(&mut self.compare, element_low, element_low_pred) < 0.0;
        let mut previous = element_low;
        let mut idx = low + 1;
        while idx < high {
            let current = self.at(idx);
            let order = order(&mut self.compare, current, previous);
            if descending {
                if order >= 0.0 {
                    break;
                }
            } else if order < 0.0 {
                break;
            }
            previous = current;
            run += 1;
            idx += 1;
        }
        if descending {
            self.work[low_arg as usize..(low_arg + run) as usize].reverse();
        }
        run
    }

    /// `RunInvariantEstablished`: run n - 2 is longer than runs n - 1 and n together.
    fn invariant(&self, n: isize) -> bool {
        if n < 2 {
            return true;
        }
        let length = |i: isize| self.runs[i as usize].length;
        length(n - 2) > length(n - 1) + length(n)
    }

    /// `MergeCollapse`: merge until the run stack's invariants hold again.
    fn merge_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() as isize - 2;
            let length = |sort: &Self, i: isize| sort.runs[i as usize].length;
            if !self.invariant(n + 1) || !self.invariant(n) {
                if length(self, n - 1) < length(self, n + 1) {
                    n -= 1;
                }
                self.merge_at(n);
            } else if length(self, n) <= length(self, n + 1) {
                self.merge_at(n);
            } else {
                break;
            }
        }
    }

    /// `MergeForceCollapse`: merge every run left on the stack.
    fn merge_force_collapse(&mut self) {
        while self.runs.len() > 1 {
            let mut n = self.runs.len() as isize - 2;
            if n > 0 && self.runs[(n - 1) as usize].length < self.runs[(n + 1) as usize].length {
                n -= 1;
            }
            self.merge_at(n);
        }
    }

    /// `MergeAt`: merge runs i and i + 1 of the stack.
    fn merge_at(&mut self, i: isize) {
        let i = i as usize;
        let Run {
            base: mut base_a,
            length: mut length_a,
        } = self.runs[i];
        let Run {
            base: base_b,
            length: mut length_b,
        } = self.runs[i + 1];
        self.runs[i].length = length_a + length_b;
        self.runs.remove(i + 1);

        // Elements of a before where b's first element goes are already in place.
        let k = gallop_right(
            &mut self.compare,
            self.work,
            self.work[base_b as usize],
            base_a,
            length_a,
            0,
        );
        base_a += k;
        length_a -= k;
        if length_a == 0 {
            return;
        }
        // Elements of b after where a's last element goes are already in place.
        length_b = gallop_left(
            &mut self.compare,
            self.work,
            self.work[(base_a + length_a - 1) as usize],
            base_b,
            length_b,
            length_b - 1,
        );
        if length_b == 0 {
            return;
        }
        if length_a <= length_b {
            self.merge_low(base_a, length_a, base_b, length_b);
        } else {
            self.merge_high(base_a, length_a, base_b, length_b);
        }
    }

    /// `MergeLow`: merge a (the shorter run, copied out) with b, front to back.
    fn merge_low(
        &mut self,
        base_a: isize,
        mut length_a: isize,
        base_b: isize,
        mut length_b: isize,
    ) {
        self.temp.clear();
        self.temp
            .extend_from_slice(&self.work[base_a as usize..(base_a + length_a) as usize]);
        let Self {
            work,
            temp,
            compare,
            min_gallop: state_min_gallop,
            ..
        } = self;
        let work: &mut [T] = work;
        let mut dest = base_a;
        let mut cursor_temp = 0;
        let mut cursor_b = base_b;
        work[dest as usize] = work[cursor_b as usize];
        dest += 1;
        cursor_b += 1;

        let end = 'merge: {
            length_b -= 1;
            if length_b == 0 {
                break 'merge End::Succeed;
            }
            if length_a == 1 {
                break 'merge End::CopyOther;
            }
            let mut min_gallop = *state_min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                // One pair at a time, until one run wins min_gallop times in a row.
                loop {
                    let b = work[cursor_b as usize];
                    let a = temp[cursor_temp as usize];
                    if order(compare, b, a) < 0.0 {
                        work[dest as usize] = b;
                        dest += 1;
                        cursor_b += 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        work[dest as usize] = a;
                        dest += 1;
                        cursor_temp += 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 1 {
                            break 'merge End::CopyOther;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    }
                }
                // Gallop until neither run wins MIN_GALLOP_WINS in a row.
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = (min_gallop - 1).max(1);
                    *state_min_gallop = min_gallop;

                    let key = work[cursor_b as usize];
                    wins_a = gallop_right(compare, temp, key, cursor_temp, length_a, 0);
                    if wins_a > 0 {
                        copy(temp, cursor_temp, work, dest, wins_a);
                        dest += wins_a;
                        cursor_temp += wins_a;
                        length_a -= wins_a;
                        if length_a == 1 {
                            break 'merge End::CopyOther;
                        }
                        // Impossible with a consistent comparator, but V8 cannot assume one.
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    work[dest as usize] = work[cursor_b as usize];
                    dest += 1;
                    cursor_b += 1;
                    length_b -= 1;
                    if length_b == 0 {
                        break 'merge End::Succeed;
                    }

                    let key = temp[cursor_temp as usize];
                    wins_b = gallop_left(compare, work, key, cursor_b, length_b, 0);
                    if wins_b > 0 {
                        shift(work, cursor_b, dest, wins_b);
                        dest += wins_b;
                        cursor_b += wins_b;
                        length_b -= wins_b;
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    work[dest as usize] = temp[cursor_temp as usize];
                    dest += 1;
                    cursor_temp += 1;
                    length_a -= 1;
                    if length_a == 1 {
                        break 'merge End::CopyOther;
                    }
                }
                // Penalize leaving galloping mode.
                min_gallop += 1;
                *state_min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if length_a > 0 {
                    copy(temp, cursor_temp, work, dest, length_a);
                }
            }
            End::CopyOther => {
                // The last element of a belongs at the end of the merge.
                shift(work, cursor_b, dest, length_b);
                work[(dest + length_b) as usize] = temp[cursor_temp as usize];
            }
        }
    }

    /// `MergeHigh`: merge a with b (the shorter run, copied out), back to front.
    fn merge_high(
        &mut self,
        base_a: isize,
        mut length_a: isize,
        base_b: isize,
        mut length_b: isize,
    ) {
        self.temp.clear();
        self.temp
            .extend_from_slice(&self.work[base_b as usize..(base_b + length_b) as usize]);
        let Self {
            work,
            temp,
            compare,
            min_gallop: state_min_gallop,
            ..
        } = self;
        let work: &mut [T] = work;
        let mut dest = base_b + length_b - 1;
        let mut cursor_temp = length_b - 1;
        let mut cursor_a = base_a + length_a - 1;
        work[dest as usize] = work[cursor_a as usize];
        dest -= 1;
        cursor_a -= 1;

        let end = 'merge: {
            length_a -= 1;
            if length_a == 0 {
                break 'merge End::Succeed;
            }
            if length_b == 1 {
                break 'merge End::CopyOther;
            }
            let mut min_gallop = *state_min_gallop;
            loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let b = temp[cursor_temp as usize];
                    let a = work[cursor_a as usize];
                    if order(compare, b, a) < 0.0 {
                        work[dest as usize] = a;
                        dest -= 1;
                        cursor_a -= 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        work[dest as usize] = b;
                        dest -= 1;
                        cursor_temp -= 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 1 {
                            break 'merge End::CopyOther;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first {
                    first = false;
                    min_gallop = (min_gallop - 1).max(1);
                    *state_min_gallop = min_gallop;

                    let key = temp[cursor_temp as usize];
                    let k = gallop_right(compare, work, key, base_a, length_a, length_a - 1);
                    wins_a = length_a - k;
                    if wins_a > 0 {
                        dest -= wins_a;
                        cursor_a -= wins_a;
                        shift(work, cursor_a + 1, dest + 1, wins_a);
                        length_a -= wins_a;
                        if length_a == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    work[dest as usize] = temp[cursor_temp as usize];
                    dest -= 1;
                    cursor_temp -= 1;
                    length_b -= 1;
                    if length_b == 1 {
                        break 'merge End::CopyOther;
                    }

                    let key = work[cursor_a as usize];
                    let k = gallop_left(compare, temp, key, 0, length_b, length_b - 1);
                    wins_b = length_b - k;
                    if wins_b > 0 {
                        dest -= wins_b;
                        cursor_temp -= wins_b;
                        copy(temp, cursor_temp + 1, work, dest + 1, wins_b);
                        length_b -= wins_b;
                        if length_b == 1 {
                            break 'merge End::CopyOther;
                        }
                        // Impossible with a consistent comparator, but V8 cannot assume one.
                        if length_b == 0 {
                            break 'merge End::Succeed;
                        }
                    }
                    work[dest as usize] = work[cursor_a as usize];
                    dest -= 1;
                    cursor_a -= 1;
                    length_a -= 1;
                    if length_a == 0 {
                        break 'merge End::Succeed;
                    }
                }
                min_gallop += 1;
                *state_min_gallop = min_gallop;
            }
        };
        match end {
            End::Succeed => {
                if length_b > 0 {
                    copy(temp, 0, work, dest - (length_b - 1), length_b);
                }
            }
            End::CopyOther => {
                // The first element of b belongs at the front of the merge.
                dest -= length_a;
                cursor_a -= length_a;
                shift(work, cursor_a + 1, dest + 1, length_a);
                work[dest as usize] = temp[cursor_temp as usize];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sort_by;

    /// With a consistent comparator every stable sort agrees, so the port must match Rust's own
    /// stable sort. Sizes up to 2,000 and few distinct keys reach the merges and galloping.
    #[test]
    fn matches_a_stable_sort_when_the_comparator_is_consistent() {
        let mut state = 0x2545_F491_u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for &n in &[0, 1, 2, 3, 63, 64, 65, 100, 257, 1000, 2000] {
            for keys in [2, 10, 1000] {
                for pattern in 0..3 {
                    let mut v: Vec<(u32, usize)> = (0..n)
                        .map(|i| {
                            let key = match pattern {
                                0 => next() % keys,
                                1 => (i as u32 / 7) % keys, // ascending runs with ties
                                _ => (n - i) as u32 / 3 % keys, // descending runs
                            };
                            (key, i)
                        })
                        .collect();
                    let mut expected = v.clone();
                    expected.sort_by_key(|&(key, _)| key);
                    sort_by(&mut v, |a, b| f64::from(a.0) - f64::from(b.0));
                    assert_eq!(v, expected, "n {n}, keys {keys}, pattern {pattern}");
                }
            }
        }
    }
}
