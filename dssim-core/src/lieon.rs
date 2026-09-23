//! Shim for single-threaded rayon replacement
//!
//! Only the pieces of rayon's API surface this crate actually calls are
//! provided; add a trait back if a new call site needs it.

// Unlike other code in this project, this file is licensed
// under both CC0 and AGPL-3.0, whichever you prefer.
// <https://creativecommons.org/public-domain/cc0/>

pub mod prelude {
    pub use super::*;
}

pub trait ParIterator: Sized {
    fn with_max_len(self, _one: usize) -> Self { self }
    fn par_bridge(self) -> Self { self }
}

impl<T: Iterator> ParIterator for T {
}

pub trait ParSliceMutLie<T> {
    fn par_chunks_exact_mut(&mut self, n: usize) -> std::slice::ChunksExactMut<'_, T>;
    /// Consumes the `&mut [T]` so callers need no `mut` binding.
    fn par_chunks_mut<'a>(self, n: usize) -> std::slice::ChunksMut<'a, T>
    where
        Self: 'a;
}

pub trait ParIntoIterLie<T> {
    type IntoIter;
    fn into_par_iter(self) -> Self::IntoIter;
}

pub fn join<A, B>(a: impl FnOnce() -> A, b: impl FnOnce() -> B) -> (A, B) {
    let a = a();
    let b = b();
    (a, b)
}

impl<'a, T> ParSliceMutLie<T> for &'a mut [T] {
    fn par_chunks_exact_mut(&mut self, n: usize) -> std::slice::ChunksExactMut<'_, T> {
        self.chunks_exact_mut(n)
    }

    fn par_chunks_mut<'x>(self, n: usize) -> std::slice::ChunksMut<'x, T>
    where
        Self: 'x,
    {
        self.chunks_mut(n)
    }
}

impl<T> ParIntoIterLie<T> for Vec<T> {
    type IntoIter = std::vec::IntoIter<T>;

    fn into_par_iter(self) -> Self::IntoIter {
        self.into_iter()
    }
}

impl ParIntoIterLie<usize> for std::ops::Range<usize> {
    type IntoIter = Self;

    fn into_par_iter(self) -> Self::IntoIter {
        self
    }
}
