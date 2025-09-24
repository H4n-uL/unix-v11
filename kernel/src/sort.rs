use wikisort::*;
use core::cmp::Ordering;

pub trait HeaplessSort<T> {
    fn sort_noheap(&mut self) where T: Ord;
    fn sort_noheap_by<F>(&mut self, cmp: F) where F: Fn(&T, &T) -> Ordering;
    fn sort_noheap_by_key<F, K>(&mut self, f: F) where F: Fn(&T) -> K, K: Ord;
}

impl<T> HeaplessSort<T> for [T] {
    fn sort_noheap(&mut self) where T: Ord {
        wikisort(self, |a, b| a.cmp(b));
    }

    fn sort_noheap_by<F>(&mut self, cmp: F) where F: Fn(&T, &T) -> Ordering {
        wikisort(self, cmp);
    }

    fn sort_noheap_by_key<F, K>(&mut self, f: F) where F: Fn(&T) -> K, K: Ord {
        wikisort(self, |a, b| f(a).cmp(&f(b)));
    }
}
