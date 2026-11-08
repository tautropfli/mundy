use std::{error, fmt};

/// A collection of callbacks optimized for
/// iteration and adding while being slow on removal.
#[derive(Debug)]
pub(crate) struct Callbacks<T> {
    callbacks: Vec<Callback<T>>,
    next_handle: usize,
}

impl<T> Callbacks<T> {
    pub(crate) const fn new() -> Self {
        /// The first handle is 1 and not 0 so that
        /// we can have an "invalid" handle that never matches anything.
        const FIRST_HANDLE: usize = 1;
        Self {
            callbacks: Vec::new(),
            next_handle: FIRST_HANDLE,
        }
    }

    #[cfg(target_os = "android")]
    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.callbacks.iter().map(|c| &c.value)
    }

    pub(crate) fn add(&mut self, value: T) -> Result<CallbackHandle, OverflowError> {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.checked_add(1).ok_or(OverflowError)?;
        self.callbacks.push(Callback { handle, value });
        Ok(CallbackHandle(handle))
    }

    pub(crate) fn remove(&mut self, handle: CallbackHandle) {
        self.callbacks.retain(|c| c.handle != handle.0);
    }

    /// Looks up a callback by the [`CallbackHandle::id`] of its handle.
    /// Handles are never reused, so the id of a removed callback finds nothing.
    #[cfg(any(all(windows, feature = "power-saver"), test))]
    pub(crate) fn get(&self, id: usize) -> Option<&T> {
        self.callbacks
            .iter()
            .find(|c| c.handle == id)
            .map(|c| &c.value)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OverflowError;

impl fmt::Display for OverflowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to add callback: no more handles left")
    }
}

impl error::Error for OverflowError {}

/// An opaque handle used to remove a callback.
#[derive(Debug, Default)]
#[must_use]
pub(crate) struct CallbackHandle(usize);

impl CallbackHandle {
    /// An id for passing the handle through an API that takes an opaque value, such as a C callback's context.
    #[cfg(any(all(windows, feature = "power-saver"), test))]
    pub(crate) fn id(&self) -> usize {
        self.0
    }
}

#[derive(Debug)]
struct Callback<T> {
    handle: usize,
    value: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_an_entry() {
        let mut callbacks = Callbacks::new();
        let _foo = callbacks.add("foo");
        assert_eq!(vec![&"foo"], callbacks.iter().collect::<Vec<_>>());
    }

    #[test]
    fn adds_multiple_entries() {
        let mut callbacks = Callbacks::new();
        let _foo = callbacks.add("foo").expect("no overflow");
        let _bar = callbacks.add("bar").expect("no overflow");
        let _baz = callbacks.add("baz").expect("no overflow");
        assert_eq!(
            vec![&"foo", &"bar", &"baz"],
            callbacks.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn removes_an_entry() {
        let mut callbacks = Callbacks::new();
        let _foo = callbacks.add("foo").expect("no overflow");
        let bar = callbacks.add("bar").expect("no overflow");
        let _baz = callbacks.add("baz").expect("no overflow");
        callbacks.remove(bar);
        assert_eq!(vec![&"foo", &"baz"], callbacks.iter().collect::<Vec<_>>());
    }

    #[test]
    fn removes_all_entry() {
        let mut callbacks = Callbacks::new();
        let foo = callbacks.add("foo").expect("no overflow");
        let bar = callbacks.add("bar").expect("no overflow");
        let baz = callbacks.add("baz").expect("no overflow");
        callbacks.remove(bar);
        callbacks.remove(baz);
        callbacks.remove(foo);
        assert_eq!(0, callbacks.iter().count());
    }

    #[test]
    fn gets_an_entry_by_id_until_it_is_removed() {
        let mut callbacks = Callbacks::new();
        let foo = callbacks.add("foo").expect("no overflow");
        let bar = callbacks.add("bar").expect("no overflow");
        let (foo_id, bar_id) = (foo.id(), bar.id());
        assert_eq!(Some(&"foo"), callbacks.get(foo_id));
        assert_eq!(Some(&"bar"), callbacks.get(bar_id));
        assert_eq!(None, callbacks.get(CallbackHandle::default().id()));
        callbacks.remove(foo);
        assert_eq!(None, callbacks.get(foo_id));
        assert_eq!(Some(&"bar"), callbacks.get(bar_id));
        let _baz = callbacks.add("baz").expect("no overflow");
        assert_eq!(None, callbacks.get(foo_id), "ids are not reused");
    }

    #[test]
    fn default_handle_does_not_remove_first_entry() {
        let mut callbacks = Callbacks::new();
        let _foo = callbacks.add("foo").expect("no overflow");
        callbacks.remove(CallbackHandle::default());
        assert_eq!(vec![&"foo"], callbacks.iter().collect::<Vec<_>>());
    }
}
