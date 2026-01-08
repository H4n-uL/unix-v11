use crate::arch::inter;

use core::{mem::ManuallyDrop, ops::{Deref, DerefMut}};
use lock_api::{RawMutex, RawRwLock};

pub struct IntLock<R: RawMutex, T> {
    mutex: lock_api::Mutex<R, T>,
}

impl<R: RawMutex, T> IntLock<R, T> {
    pub const fn new(data: T) -> Self {
        Self { mutex: lock_api::Mutex::const_new(<R as RawMutex>::INIT, data) }
    }

    pub fn lock(&self) -> IntLockGuard<'_, R, T> {
        let inter = inter::get();
        inter::set(false);
        IntLockGuard {
            guard: ManuallyDrop::new(self.mutex.lock()),
            inter,
        }
    }

    // pub fn try_lock(&self) -> Option<IntLockGuard<'_, R, T>> {
    //     let inter = inter::get();
    //     inter::set(false);
    //     match self.mutex.try_lock() {
    //         Some(guard) => Some(IntLockGuard {
    //             guard: ManuallyDrop::new(guard),
    //             inter,
    //         }),
    //         None => {
    //             inter::set(inter);
    //             None
    //         }
    //     }
    // }
}

pub struct IntLockGuard<'a, R: RawMutex, T> {
    guard: ManuallyDrop<lock_api::MutexGuard<'a, R, T>>,
    inter: bool,
}

impl<R: RawMutex, T> Deref for IntLockGuard<'_, R, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.guard }
}

impl<R: RawMutex, T> DerefMut for IntLockGuard<'_, R, T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.guard }
}

impl<R: RawMutex, T> Drop for IntLockGuard<'_, R, T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.guard); }
        inter::set(self.inter);
    }
}

pub struct IntRwLock<R: RawRwLock, T> {
    mutex: lock_api::RwLock<R, T>,
}

impl<R: RawRwLock, T> IntRwLock<R, T> {
    pub const fn new(data: T) -> Self {
        Self { mutex: lock_api::RwLock::const_new(<R as RawRwLock>::INIT, data) }
    }

    pub fn read(&self) -> IntRwReadGuard<'_, R, T> {
        let inter = inter::get();
        inter::set(false);
        IntRwReadGuard { guard: ManuallyDrop::new(self.mutex.read()), inter }
    }

    pub fn write(&self) -> IntRwWriteGuard<'_, R, T> {
        let inter = inter::get();
        inter::set(false);
        IntRwWriteGuard { guard: ManuallyDrop::new(self.mutex.write()), inter }
    }
}

pub struct IntRwReadGuard<'a, R: RawRwLock, T> {
    guard: ManuallyDrop<lock_api::RwLockReadGuard<'a, R, T>>,
    inter: bool,
}

impl<R: RawRwLock, T> Deref for IntRwReadGuard<'_, R, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.guard }
}

impl<R: RawRwLock, T> Drop for IntRwReadGuard<'_, R, T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.guard); }
        inter::set(self.inter);
    }
}

pub struct IntRwWriteGuard<'a, R: RawRwLock, T> {
    guard: ManuallyDrop<lock_api::RwLockWriteGuard<'a, R, T>>,
    inter: bool,
}

impl<R: RawRwLock, T> Deref for IntRwWriteGuard<'_, R, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.guard }
}

impl<R: RawRwLock, T> DerefMut for IntRwWriteGuard<'_, R, T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.guard }
}

impl<R: RawRwLock, T> Drop for IntRwWriteGuard<'_, R, T> {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.guard); }
        inter::set(self.inter);
    }
}
