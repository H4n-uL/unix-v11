use core::cmp::Ordering;

pub trait HeaplessSort<T> {
    fn sort_noheap(&mut self) where T: Ord;
    fn sort_noheap_by<F>(&mut self, cmp: F) where F: FnMut(&T, &T) -> Ordering;
    fn sort_noheap_by_key<F, K>(&mut self, key: F) where F: FnMut(&T) -> K, K: Ord;
}

impl<T> HeaplessSort<T> for &mut [T] {
    fn sort_noheap(&mut self) where T: Ord {
        if self.len() <= 1 { return; }
        self.sort_noheap_by(&mut |a: &T, b: &T| a.cmp(b));
    }

    fn sort_noheap_by<F>(&mut self, mut cmp: F)
    where F: FnMut(&T, &T) -> Ordering {
        if self.len() <= 1 { return; }
        block_merge_sort(self, &mut cmp);
    }

    fn sort_noheap_by_key<F, K>(&mut self, mut key: F)
    where F: FnMut(&T) -> K, K: Ord {
        self.sort_noheap_by(|a, b| key(a).cmp(&key(b)));
    }
}

fn block_merge_sort<T, F>(arr: &mut [T], cmp: &mut F)
where F: FnMut(&T, &T) -> Ordering {
    let len = arr.len();
    if len <= 16 { insertion_sort(arr, cmp); return; }
    let block_size = isqrt(len).max(16);

    let mut width = 1;
    while width < len {
        let mut start = 0;
        while start < len {
            let mid = (start + width).min(len);
            let end = (start + 2 * width).min(len);
            if mid < end { block_merge(arr, start, mid, end, block_size, cmp); }
            start += 2 * width;
        }
        width *= 2;
    }
}

fn block_merge<T, F>(arr: &mut [T], mut start: usize, mut mid: usize, end: usize, block_size: usize, cmp: &mut F)
where F: FnMut(&T, &T) -> Ordering {
    loop {
        let left_len = mid - start;
        let right_len = end - mid;
        if left_len == 0 || right_len == 0 { return };

        let mut left_pos = start;
        let mut right_pos = mid;
        let mut should_break = true;

        while left_pos < mid && right_pos < end {
            let left_block_end = (left_pos + block_size).min(mid);
            let right_block_end = (right_pos + block_size).min(end);

            if cmp(&arr[left_pos], &arr[right_pos]) != Ordering::Greater { left_pos = left_block_end; }
            else {
                rotate_merge(arr, left_pos, right_pos, right_block_end, cmp);
                let moved_len = right_block_end - right_pos;
                left_pos += moved_len;
                right_pos = right_block_end;
                let new_mid = mid + moved_len;
                if new_mid <= end {
                    (start, mid, should_break) = (left_pos, new_mid, false);
                    break;
                }
            }
        }

        if should_break { break };
    }
}

fn rotate_merge<T, F>(arr: &mut [T], start: usize, mid: usize, end: usize, cmp: &mut F)
where F: FnMut(&T, &T) -> Ordering {
    let mut left = start;
    let mut right = mid;

    while left < right && right < end {
        while left < right && cmp(&arr[left], &arr[right]) != Ordering::Greater { left += 1; }
        if left == right { break; }

        let mut right_end = right + 1;
        while right_end < end && cmp(&arr[left], &arr[right_end]) == Ordering::Greater { right_end += 1; }

        reverse_range(arr, left, right);
        reverse_range(arr, right, right_end);
        reverse_range(arr, left, right_end);

        let moved_len = right_end - right;
        left += moved_len;
        right = right_end;
    }
}

fn reverse_range<T>(arr: &mut [T], start: usize, end: usize) {
    if start >= end { return; }
    let mut left = start;
    let mut right = end - 1;
    while left < right {
        arr.swap(left, right);
        left += 1;
        right -= 1;
    }
}

fn insertion_sort<T, F>(arr: &mut [T], cmp: &mut F)
where F: FnMut(&T, &T) -> Ordering {
    for i in 1..arr.len() {
        let mut j = i;
        while j > 0 && cmp(&arr[j - 1], &arr[j]) == Ordering::Greater {
            arr.swap(j - 1, j); j -= 1;
        }
    }
}

fn isqrt(n: usize) -> usize {
    if n < 2 { return n; }
    let mut x = 1 << ((n.ilog2() + 1) / 2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x { return x; } x = y;
    }
}