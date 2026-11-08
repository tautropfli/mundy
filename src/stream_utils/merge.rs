use futures_lite::stream::{Fuse, Stream, StreamExt as _};
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// Yields the items of both streams as they arrive and ends once both streams have ended.
    pub(crate) struct Merge<A, B> {
        #[pin]
        a: Fuse<A>,
        #[pin]
        b: Fuse<B>,
    }
}

impl<A: Stream, B: Stream<Item = A::Item>> Merge<A, B> {
    pub(crate) fn new(a: A, b: B) -> Self {
        Merge {
            a: a.fuse(),
            b: b.fuse(),
        }
    }
}

impl<A: Stream, B: Stream<Item = A::Item>> Stream for Merge<A, B> {
    type Item = A::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        let a = this.a.poll_next(cx);
        if let Poll::Ready(Some(item)) = a {
            return Poll::Ready(Some(item));
        }
        match (a, this.b.poll_next(cx)) {
            (_, Poll::Ready(Some(item))) => Poll::Ready(Some(item)),
            (Poll::Ready(None), Poll::Ready(None)) => Poll::Ready(None),
            _ => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_lite::future::block_on;
    use futures_lite::stream;

    #[test]
    fn yields_the_items_of_both_streams() {
        let merged = Merge::new(stream::iter([1, 2]), stream::iter([3]));
        assert_eq!(vec![1, 2, 3], block_on(merged.collect::<Vec<_>>()));
    }

    #[test]
    fn continues_after_one_stream_ends() {
        let merged = Merge::new(stream::empty(), stream::iter([1, 2]));
        assert_eq!(vec![1, 2], block_on(merged.collect::<Vec<_>>()));
        let merged = Merge::new(stream::iter([1, 2]), stream::empty());
        assert_eq!(vec![1, 2], block_on(merged.collect::<Vec<_>>()));
    }

    #[test]
    fn ends_when_both_streams_end() {
        let mut merged = Merge::new(stream::empty::<i32>(), stream::empty());
        assert_eq!(None, block_on(merged.next()));
    }

    #[test]
    fn does_not_end_while_a_stream_is_pending() {
        let mut merged = Merge::new(stream::empty::<i32>(), stream::pending());
        let poll = block_on(futures_lite::future::poll_once(merged.next()));
        assert_eq!(None, poll);
    }
}
