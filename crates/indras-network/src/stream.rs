//! Stream utilities for reactive data.
//!
//! Provides utilities for converting between broadcast channels
//! and async streams for ergonomic event handling.

use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;

/// Convert a broadcast receiver into an async Stream.
///
/// This handles the `Lagged` error by continuing to receive
/// subsequent messages (older messages are lost).
pub fn broadcast_to_stream<T: Clone + Send + 'static>(
    rx: broadcast::Receiver<T>,
) -> impl Stream<Item = T> + Send {
    BroadcastStream { rx }
}

/// Stream wrapper for broadcast receiver.
struct BroadcastStream<T> {
    rx: broadcast::Receiver<T>,
}

impl<T: Clone + Send> Stream for BroadcastStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::future::Future;

        // Create a future for receiving
        let recv_future = self.rx.recv();
        tokio::pin!(recv_future);

        match recv_future.poll(cx) {
            Poll::Ready(Ok(item)) => Poll::Ready(Some(item)),
            Poll::Ready(Err(broadcast::error::RecvError::Lagged(_))) => {
                // Lagged - we missed some messages, but continue receiving
                // Wake up to try again
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Err(broadcast::error::RecvError::Closed)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Create an async stream using a generator-like syntax.
///
/// This is a convenience macro for creating streams that yield values.
#[macro_export]
macro_rules! stream {
    ($($body:tt)*) => {
        async_stream::stream! { $($body)* }
    };
}

/// An update from a realm that can be either a message or presence event.
#[derive(Debug, Clone)]
pub enum RealmUpdate<M, P> {
    /// A new message was received.
    Message(M),
    /// A presence event occurred.
    Presence(P),
}

/// Combine two streams into one, tagging items with their source.
pub fn select_updates<M, P>(
    messages: impl Stream<Item = M> + Send + 'static,
    presence: impl Stream<Item = P> + Send + 'static,
) -> impl Stream<Item = RealmUpdate<M, P>> + Send
where
    M: Send + 'static,
    P: Send + 'static,
{
    use futures::StreamExt;

    futures::stream::select(
        messages.map(RealmUpdate::Message),
        presence.map(RealmUpdate::Presence),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_broadcast_to_stream() {
        let (tx, rx) = broadcast::channel::<i32>(16);

        let mut stream = broadcast_to_stream(rx);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        drop(tx);

        let items: Vec<_> = stream.collect().await;
        assert_eq!(items, vec![1, 2, 3]);
    }
}
