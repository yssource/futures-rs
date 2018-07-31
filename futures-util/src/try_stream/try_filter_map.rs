use core::marker::Unpin;
use core::mem::PinMut;
use futures_core::future::{TryFuture};
use futures_core::stream::{Stream, TryStream};
use futures_core::task::{self, Poll};

/// A combinator that attempts to filter the results of a stream
/// and simultaneously map them to a different type.
///
/// This structure is returned by the
/// [`try_filter_map`](super::TryStreamExt::try_filter_map) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct TryFilterMap<St, Fut, F> {
    stream: St,
    f: F,
    pending: Option<Fut>,
}

impl<St, Fut, F> Unpin for TryFilterMap<St, Fut, F>
    where St: Unpin, Fut: Unpin,
{}

impl<St, Fut, F> TryFilterMap<St, Fut, F> {
    unsafe_pinned!(stream: St);
    unsafe_unpinned!(f: F);
    unsafe_pinned!(pending: Option<Fut>);

    pub(super) fn new(stream: St, f: F) -> Self {
        TryFilterMap { stream, f, pending: None }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the
    /// stream which may otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so
    /// care should be taken to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St, Fut, F, T> Stream for TryFilterMap<St, Fut, F>
    where St: TryStream,
          Fut: TryFuture<Ok = Option<T>, Error = St::Error>,
          F: FnMut(St::Ok) -> Fut,
{
    type Item = Result<T, St::Error>;

    fn poll_next(
        mut self: PinMut<Self>,
        cx: &mut task::Context,
    ) -> Poll<Option<Result<T, St::Error>>> {
        loop {
            if self.pending().as_pin_mut().is_none() {
                let item = match ready!(self.stream().try_poll_next(cx)) {
                    Some(Ok(x)) => x,
                    Some(Err(e)) => return Poll::Ready(Some(Err(e))),
                    None => return Poll::Ready(None),
                };
                let fut = (self.f())(item);
                PinMut::set(self.pending(), Some(fut));
            }

            let result = ready!(self.pending().as_pin_mut().unwrap().try_poll(cx));
            PinMut::set(self.pending(), None);
            match result {
                Ok(Some(x)) => return Poll::Ready(Some(Ok(x))),
                Err(e) => return Poll::Ready(Some(Err(e))),
                Ok(None) => {},
            }
        }
    }
}

