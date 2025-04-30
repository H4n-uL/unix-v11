use core::cmp::Ordering;

pub trait HeaplessSort<T> {
    fn sort_noheap_by<F>(&mut self, cmp: F) where F: FnMut(&T, &T) -> Ordering;
    fn sort_noheap_by_key<F, K>(&mut self, key: F) where F: FnMut(&T) -> K, K: Ord;
}

impl<T> HeaplessSort<T> for &mut [T] {
    fn sort_noheap_by<F>(&mut self, mut cmp: F)
    where F: FnMut(&T, &T) -> Ordering {
        let len = self.len();
        for i in 1..len {
            let mut j = i;
            while j > 0 && cmp(&self[j - 1], &self[j]) == Ordering::Greater {
                self.swap(j - 1, j); j -= 1;
            }
        }
    }

    fn sort_noheap_by_key<F, K>(&mut self, mut key: F)
    where F: FnMut(&T) -> K, K: Ord {
        let len = self.len();
        for i in 1..len {
            let mut j = i;
            while j > 0 && key(&self[j - 1]) > key(&self[j]) {
                self.swap(j - 1, j); j -= 1;
            }
        }
    }
}