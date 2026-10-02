pub mod client;
mod lru_cache;
pub mod transport;

#[cfg(feature = "http-blocking")]
pub mod http_blocking;

#[cfg(feature = "http-async")]
pub mod http_async;

#[cfg(feature = "grpc")]
pub mod grpc;

pub use client::{AsyncFlagrantClient, Features, FlagrantClient};

#[cfg(feature = "http-blocking")]
pub use http_blocking::HttpBlockingTransport;

#[cfg(feature = "http-async")]
pub use http_async::HttpAsyncTransport;

#[cfg(feature = "grpc")]
pub use grpc::GrpcTransport;
