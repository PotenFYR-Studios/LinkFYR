//! Fixed-capacity ring buffer for time-series samples.
//! Bounded by design: telemetry must never grow without limit.

use std::num::NonZeroUsize;

#[derive(Debug)]
pub struct RingBuffer<T> {
    data: std::collections::VecDeque<T>,
    cap: NonZeroUsize,
}

impl<T> RingBuffer<T> {
    pub fn new(cap: NonZeroUsize) -> Self {
        Self {
            data: std::collections::VecDeque::with_capacity(cap.get()),
            cap,
        }
    }

    pub fn push(&mut self, value: T) {
        if self.data.len() == self.cap.get() {
            self.data.pop_front();
        }
        self.data.push_back(value);
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, T> {
        self.data.iter()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn last(&self) -> Option<&T> {
        self.data.back()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl<'a, T> IntoIterator for &'a RingBuffer<T> {
    type Item = &'a T;
    type IntoIter = std::collections::vec_deque::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    #[test]
    fn evicts_oldest_when_full() {
        let mut r: RingBuffer<u32> = RingBuffer::new(NonZeroUsize::new(3).unwrap());
        for v in [1, 2, 3, 4, 5] {
            r.push(v);
        }
        let collected: Vec<u32> = r.iter().copied().collect();
        assert_eq!(collected, vec![3, 4, 5]);
        assert_eq!(r.len(), 3);
        assert_eq!(r.last(), Some(&5));
    }

    #[test]
    fn stays_bounded_after_many_pushes() {
        let mut r: RingBuffer<u64> = RingBuffer::new(NonZeroUsize::new(60).unwrap());
        for v in 0..10_000u64 {
            r.push(v);
        }
        assert_eq!(r.len(), 60);
    }

    #[test]
    fn empty_state_is_visible() {
        let r: RingBuffer<u32> = RingBuffer::new(NonZeroUsize::new(4).unwrap());
        assert!(r.is_empty());
        assert_eq!(r.last(), None);
    }
}
