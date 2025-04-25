## RabbitMQ Streammer

A library for streaming messages from RabbitMQ. It provides a high-level API for connecting to RabbitMQ and consuming messages from queues.

### Features

- **RabbitConsumer**: A struct representing a RabbitMQ consumer that connects to a RabbitMQ broker, binds the specified queue to an exchange, and loads messages from the specified queue.
  - **Parameters**:
    - `channel`: A reference-counted handle to a channel used to receive messages.
    - `queue_name`: The name of the queue where messages will be consumed.
    - `app_group_namespace`: An identifier for the application used to create routing keys, which helps in segregating messages based on different applications or groups.

- **RabbitPublisher**: A struct representing a RabbitMQ publisher that connects to a RabbitMQ broker, binds the specified exchange, and publishes messages to it.
  - **Parameters**:
    - `channel`: A reference-counted handle to a channel used to send messages.
    - `exchange_name`: The name of the exchange where messages will be published.
    - `app_group_namespace`: An identifier for the application used to create routing keys, which helps in segregating messages based on different applications or groups.

### Examples

#### Setting Up a RabbitConsumer

```rust
use rabbitmq_streammer::RabbitConsumer;
use futures::StreamExt;
use serde::Deserialize;

#[derive(Deserialize)]
struct Payload {
    data: String,
    id: u32,
}

#[tokio::main]
async fn main() {
    let uri = "amqp://admin:password@pipe:5672";
    let queue_name = "paylods";
    let app_group_namespace = "test_application";
    let exchange_name = "test";

    let consumer = RabbitConsumer::connect(uri, queue_name, app_group_namespace, exchange_name).await.unwrap();

    let mut messages = consumer.load_messages::<Vec<Payload>>(10, "test-tag").await.unwrap();

    while let Some(message) = messages.next().await {
        println!("Received message: {:?}", message);
    }
}