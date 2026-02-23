//! Stream utilities for reactive data.
//!
//! Provides utilities for converting between broadcast channels
//! and async streams for ergonomic event handling.

use std::pin::Pin;

use futures::Stream;
use tokio::sync::broadcast;

/// Convert a broadcast receiver into an async Stream.
///
/// This handles the `Lagged` error by continuing to receive
/// subsequent messages (older messages are lost).
pub fn broadcast_to_stream<T: Clone + Send + 'static>(
    mut rx: broadcast::Receiver<T>,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(item) => yield item,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
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

        let stream = broadcast_to_stream(rx);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        drop(tx);

        let items: Vec<_> = stream.collect().await;
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_broadcast_to_stream_subscribe_before_send() {
        let (tx, rx) = broadcast::channel::<i32>(16);
        let stream = broadcast_to_stream(rx);

        // Send AFTER subscribing (the real-world pattern)
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            tx.send(42).unwrap();
            drop(tx);
        });

        let items: Vec<_> = stream.collect().await;
        assert_eq!(items, vec![42]);
    }
}
