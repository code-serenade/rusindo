use crate::error::Result;
use bytes::BytesMut;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A trait that represents a handler for processing incoming messages.
///
/// The `Handler` trait defines a single method `call` which is responsible
/// for asynchronously processing the input data and returning a result.
pub trait Handler {
    /// Processes the input data and returns a future with the result.
    ///
    /// # Arguments
    ///
    /// * `data` - A `BytesMut` object containing the input data.
    ///
    /// # Returns
    ///
    /// * A pinned box containing a future that resolves to a `Result<BytesMut>`.
    fn call(&self, data: BytesMut) -> Pin<Box<dyn Future<Output = Result<BytesMut>> + Send>>;
}

impl<F, Fut> Handler for F
where
    F: Fn(BytesMut) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<BytesMut>> + Send + 'static,
{
    /// Calls the function to process the data and returns a future with the result.
    fn call(&self, data: BytesMut) -> Pin<Box<dyn Future<Output = Result<BytesMut>> + Send>> {
        Box::pin((self)(data))
    }
}

/// A router that manages a collection of routes, each associated with a command ID.
///
/// The `Router` struct holds a mapping between command IDs and their corresponding
/// handlers. It provides functionality to add routes and handle incoming messages
/// by dispatching them to the appropriate handler.
pub struct Router {
    routes: HashMap<u16, Arc<dyn Handler + Send + Sync>>,
}

impl Router {
    /// Creates a new, empty `Router`.
    ///
    /// # Returns
    ///
    /// * A new instance of `Router` with no routes registered.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Adds a route to the router.
    ///
    /// This method associates a command ID with a handler.
    ///
    /// # Arguments
    ///
    /// * `cmd` - A `u16` representing the command ID.
    /// * `handler` - The handler function or closure that will process messages
    ///               with the specified command ID.
    ///
    /// # Returns
    ///
    /// * A mutable reference to the `Router`, allowing for method chaining.
    pub fn add_route<H>(&mut self, cmd: u16, handler: H) -> &mut Self
    where
        H: Handler + Send + Sync + 'static,
    {
        self.routes.insert(cmd, Arc::new(handler));
        self
    }

    /// Handles an incoming message by dispatching it to the appropriate handler.
    ///
    /// # Arguments
    ///
    /// * `cmd` - A `u16` representing the command ID of the incoming message.
    /// * `data` - A `BytesMut` object containing the message data.
    ///
    /// # Returns
    ///
    /// * A future that resolves to a `Result<BytesMut>`, representing either
    ///   the processed response data or an error if no handler was found for
    ///   the command ID.
    pub async fn handle_message(&self, cmd: u16, data: BytesMut) -> Result<BytesMut> {
        if let Some(handler) = self.routes.get(&cmd) {
            handler.call(data).await
        } else {
            Err(crate::error::Error::ErrorCode(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use bytes::BytesMut;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[tokio::test]
    async fn test_router_add_and_handle_route() {
        let mut router = Router::new();

        // Define a handler function for a specific command ID (e.g., 42)
        router.add_route(42, |data: BytesMut| async move {
            let mut response = BytesMut::from("Echo: ");
            response.extend_from_slice(&data);
            Ok(response)
        });

        // Create test data
        let input_data = BytesMut::from("Hello, World!");

        // Handle the message with command ID 42
        let result = router.handle_message(42, input_data.clone()).await;

        // Assert that the result is correct
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BytesMut::from("Echo: Hello, World!"));

        // Test with an unknown command ID (e.g., 99)
        let result = router.handle_message(99, input_data).await;

        // Assert that the result is an error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_router_with_multiple_routes() {
        let mut router = Router::new();

        // Add two different routes
        router.add_route(
            1,
            |_: BytesMut| async move { Ok(BytesMut::from("Route 1")) },
        );

        router.add_route(
            2,
            |_: BytesMut| async move { Ok(BytesMut::from("Route 2")) },
        );

        // Test each route
        let result = router.handle_message(1, BytesMut::new()).await;
        assert_eq!(result.unwrap(), BytesMut::from("Route 1"));

        let result = router.handle_message(2, BytesMut::new()).await;
        assert_eq!(result.unwrap(), BytesMut::from("Route 2"));
    }

    #[tokio::test]
    async fn test_router_no_route() {
        let router = Router::new();

        // Test with no routes added
        let result = router.handle_message(1, BytesMut::new()).await;

        // Assert that the result is an error
        assert!(result.is_err());
    }
}
