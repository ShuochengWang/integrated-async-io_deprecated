use std::ptr::NonNull;

/// Compared to `slab::Slab`, `RawSlab` gives more control by providing unsafe APIs.
pub struct RawSlab<T> {
    buf_ptr: NonNull<T>,
    buf_len: usize,
    // TODO: use bitmap
    free_indexes: Vec<usize>,
}

impl<T> RawSlab<T> {
    /// Create a slab allocator that can allocate as most `len` number of T objects.
    pub unsafe fn new(buf_ptr: *mut T, buf_len: usize) -> Self {
        let buf_ptr = NonNull::new(buf_ptr).unwrap();
        let free_indexes = (0..buf_len).into_iter().rev().collect();
        Self {
            buf_ptr,
            buf_len,
            free_indexes,
        }
    }

    /// Allocate an object.
    ///
    /// This method is semantically equivalent to
    /// ```
    /// libc::malloc(std::mem::size_of::<T>())
    /// ```
    pub fn alloc(&mut self) -> Option<*mut T> {
        let free_index = match self.free_indexes.pop() {
            None => return None,
            Some(free_index) => free_index,
        };

        let ptr = unsafe { self.buf_ptr.as_ptr().add(free_index) };
        Some(ptr)
    }

    /// Deallocate an object.
    ///
    /// This method is semantically equivalent to
    /// ```
    /// libc::free(ptr)
    /// ```
    /// where ptr is a pointer to an object of `T` that
    /// is previously allocated by this allocator.
    ///
    /// Memory safety. The user carries the same responsibility as he would do
    /// with C's free. So use it carefully.
    pub unsafe fn dealloc(&mut self, ptr: *mut T) {
        let index = ptr.offset_from(self.buf_ptr.as_ptr()) as usize;
        debug_assert!(self.buf_ptr.as_ptr().add(index) == ptr);
        self.free_indexes.push(index);
    }

    /// Returns the max number of objects that can be allocated.
    pub fn capacity(&self) -> usize {
        self.buf_len
    }

    /// Returns the number of allocated objects.
    pub fn allocated(&self) -> usize {
        self.capacity() - self.free_indexes.len()
    }
}
