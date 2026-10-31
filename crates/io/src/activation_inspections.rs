//! Test-only storage counts elements consumed by indexed access, iteration, and membership scans.

use std::cell::Cell;
use std::iter::Inspect;
use std::ops::{Deref, Index};
use std::slice;

thread_local! {
    pub(super) static COMPONENTS: Cell<usize> = const { Cell::new(0) };
    pub(super) static MEMBERS: Cell<usize> = const { Cell::new(0) };
    pub(super) static EDGES: Cell<usize> = const { Cell::new(0) };
}

fn record<const KIND: usize>() {
    match KIND {
        0 => COMPONENTS.with(|count| count.set(count.get() + 1)),
        1 => MEMBERS.with(|count| count.set(count.get() + 1)),
        2 => EDGES.with(|count| count.set(count.get() + 1)),
        _ => panic!("unknown inspection kind"),
    }
}

fn borrowed<T, const KIND: usize>(_: &&T) {
    record::<KIND>();
}

fn owned<T, const KIND: usize>(_: &T) {
    record::<KIND>();
}

type Borrowed<'a, T> = Inspect<slice::Iter<'a, T>, fn(&&'a T)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectedVec<T, const KIND: usize>(Vec<T>);

impl<T, const KIND: usize> InspectedVec<T, KIND> {
    pub(super) fn new(values: Vec<T>) -> Self {
        Self(values)
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        let value = self.0.get(index)?;
        record::<KIND>();
        Some(value)
    }

    pub(super) fn first(&self) -> Option<&T> {
        self.get(0)
    }

    pub(super) fn iter(&self) -> Borrowed<'_, T> {
        self.0.iter().inspect(borrowed::<T, KIND> as fn(&&T))
    }

    pub(super) fn contains(&self, wanted: &T) -> bool
    where
        T: PartialEq,
    {
        self.iter().any(|value| value == wanted)
    }
}

impl<T, const KIND: usize> Deref for InspectedVec<T, KIND> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const KIND: usize> Index<usize> for InspectedVec<T, KIND> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        record::<KIND>();
        &self.0[index]
    }
}

impl<'a, T, const KIND: usize> IntoIterator for &'a InspectedVec<T, KIND> {
    type Item = &'a T;
    type IntoIter = Borrowed<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T, const KIND: usize> IntoIterator for InspectedVec<T, KIND> {
    type Item = T;
    type IntoIter = Inspect<std::vec::IntoIter<T>, fn(&T)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().inspect(owned::<T, KIND> as fn(&T))
    }
}
