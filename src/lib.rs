pub mod extensions;
pub mod rabbit_message;
pub mod consumer;
pub mod publisher;

pub use consumer::RabbitConsumer;
pub use publisher::RabbitPublisher;

#[cfg(all(feature = "lapin"))]
pub use lapin;