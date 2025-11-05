## RabbitMQ Streammer

[crates-badge]: https://img.shields.io/crates/v/rabbitmq_streamer.svg
[crates-url]: https://crates.io/crates/rabbitmq_streamer
[mit-url]: https://github.com/cgbur/rabbitmq_streamer/blob/master/LICENSE
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg

[![Crates.io][crates-badge]][crates-url](https://crates.io/crates/rabbitmq_streamer)

A easy library for publishing to RabbitMQ and Consuming from it using [Rust Streams](https://doc.rust-lang.org/book/ch17-04-streams.html). It provides a high-level API for connecting to RabbitMQ, publishing, consuming messages from queues and acking them in batches.


### Features

- **RabbitPublisher**: A high-level publisher for sending messages to RabbitMQ exchanges
  - Connect to RabbitMQ brokers with automatic exchange binding
  - Publish serializable messages with custom routing keys
  - Built-in JSON serialization for message payloads
  - Durable exchange declarations with configurable options

- **RabbitConsumer**: A stream-based consumer for receiving messages from RabbitMQ queues
  - **Auto-acknowledged messages**: Use `load_messages()` to consume messages that are automatically acknowledged upon receipt
  - **Manual acknowledgment**: Use `load_ackable_messages()` to receive `AckableMessage<T>` instances for manual message acknowledgment control
  - Automatic reconnection and error handling with configurable retry intervals
  - Built-in JSON deserialization for message payloads
  - Support for custom consumer tags and channel buffer sizing

- **AckableMessage**: Wrapper for messages requiring manual acknowledgment
  - `ack()`: Acknowledge successful message processing
  - `nack()`: Negative acknowledge with requeue option
  - `reject()`: Reject message without requeue
  - Access to the original message payload through `message()`



### Testing

This library includes comprehensive tests using testcontainers to ensure reliability with real RabbitMQ instances. The test suite covers:

- **Publisher tests**: Connection, message publishing, multiple messages, and different routing keys
- **Consumer tests**: Connection, auto-acknowledged messages, manual acknowledgment, and custom consumer tags
- **Integration tests**: End-to-end scenarios with both publisher and consumer working together

To run the tests:

```bash
# Run all tests (requires Docker)
cargo test

# Run specific test suites
cargo test --test publisher_tests
cargo test --test consumer_tests
cargo test --test integration_tests
```

For detailed testing information, see [TESTING.md](TESTING.md).

### CI/CD Pipeline

This project uses GitHub Actions for automated testing and publishing:
- **Continuous Integration** - Automatic testing on every push and PR
- **Automated Releases** - Publishes to crates.io when version changes on main branch
- **Security Audits** - Weekly dependency and security checks

For complete CI/CD documentation, see [CI_CD.md](CI_CD.md).

### Examples

#### Publishing Messages

```rust
use rabbitmq_streamer::RabbitPublisher;
use serde::Serialize;

#[derive(Serialize)]
struct OrderEvent {
    order_id: u32,
    customer_id: u32,
    amount: f64,
    status: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let uri = "amqp://guest:guest@localhost:5672";
    let exchange_name = "orders";
    let app_namespace = "order_service";

    // Connect to RabbitMQ and create publisher
    let publisher = RabbitPublisher::connect(uri, exchange_name, app_namespace).await?;

    // Create and publish a message
    let order = OrderEvent {
        order_id: 12345,
        customer_id: 67890,
        amount: 99.99,
        status: "created".to_string(),
    };

    // Publish with routing key
    publisher.publish(&order, "orders.created").await?;

    println!("Order event published successfully!");
    Ok(())
}
```

#### Consuming Messages with Auto-Acknowledgment

```rust
use rabbitmq_streamer::RabbitConsumer;
use serde::Deserialize;
use tokio::time::{timeout, Duration};

#[derive(Deserialize, Debug)]
struct OrderEvent {
    order_id: u32,
    customer_id: u32,
    amount: f64,
    status: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let uri = "amqp://guest:guest@localhost:5672";
    let queue_name = "order_events";

    // Connect to RabbitMQ and create consumer
    let consumer = RabbitConsumer::connect(uri, queue_name).await?;

    // Start consuming messages (auto-acknowledged)
    let mut message_receiver = consumer
        .load_messages::<OrderEvent, _>(10, Some("order-processor"))
        .await?;

    println!("Waiting for messages...");

    // Process messages
    while let Ok(Some(order)) = timeout(Duration::from_secs(30), message_receiver.recv()).await {
        println!("Received order: {:?}", order);
        
        // Process the order here
        // Message is automatically acknowledged
    }

    Ok(())
}
```

#### Consuming Messages with Manual Acknowledgment

```rust
use rabbitmq_streamer::RabbitConsumer;
use serde::Deserialize;
use tokio::time::{timeout, Duration};

#[derive(Deserialize, Debug)]
struct PaymentEvent {
    payment_id: u32,
    order_id: u32,
    amount: f64,
    status: String,
}

async fn process_payment(payment: &PaymentEvent) -> anyhow::Result<()> {
    // Simulate payment processing
    println!("Processing payment {} for order {}", payment.payment_id, payment.order_id);
    
    // Simulate some processing logic that might fail
    if payment.amount < 0.0 {
        return Err(anyhow::anyhow!("Invalid payment amount"));
    }
    
    // Process payment...
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let uri = "amqp://guest:guest@localhost:5672";
    let queue_name = "payment_events";

    // Connect to RabbitMQ and create consumer
    let consumer = RabbitConsumer::connect(uri, queue_name).await?;

    // Start consuming messages with manual acknowledgment
    let mut message_receiver = consumer
        .load_ackable_messages::<PaymentEvent, _>(10, Some("payment-processor"))
        .await?;

    println!("Waiting for payment events...");

    // Process messages with manual acknowledgment
    while let Ok(Some(ackable_message)) = timeout(Duration::from_secs(30), message_receiver.recv()).await {
        let payment = ackable_message.message();
        println!("Received payment: {:?}", payment);
        
        match process_payment(&payment).await {
            Ok(()) => {
                // Success: acknowledge the message
                if let Err(e) = ackable_message.ack().await {
                    eprintln!("Failed to acknowledge message: {}", e);
                }
                println!("Payment processed and acknowledged");
            }
            Err(e) => {
                eprintln!("Failed to process payment: {}", e);
                // Reject message and requeue for retry
                if let Err(e) = ackable_message.nack().await {
                    eprintln!("Failed to nack message: {}", e);
                }
            }
        }
    }

    Ok(())
}
```

#### Complete Publisher-Consumer Example

```rust
use rabbitmq_streamer::{RabbitPublisher, RabbitConsumer};
use serde::{Serialize, Deserialize};
use tokio::time::{sleep, timeout, Duration};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TaskMessage {
    task_id: u32,
    task_type: String,
    payload: String,
    created_at: u64,
}

impl TaskMessage {
    fn new(task_id: u32, task_type: &str, payload: &str) -> Self {
        Self {
            task_id,
            task_type: task_type.to_string(),
            payload: payload.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let uri = "amqp://guest:guest@localhost:5672";
    let exchange_name = "tasks";
    let queue_name = "task_queue";

    // Create publisher
    let publisher = RabbitPublisher::connect(uri, exchange_name, "task_service").await?;

    // Create consumer
    let consumer = RabbitConsumer::connect(uri, queue_name).await?;
    let mut message_receiver = consumer
        .load_ackable_messages::<TaskMessage, _>(20, Some("task-worker"))
        .await?;

    // Start consumer task
    let consumer_handle = tokio::spawn(async move {
        println!("Task worker started...");
        
        while let Ok(Some(ackable_message)) = timeout(Duration::from_secs(60), message_receiver.recv()).await {
            let task = ackable_message.message();
            println!("Processing task {}: {}", task.task_id, task.task_type);
            
            // Simulate task processing
            sleep(Duration::from_millis(500)).await;
            
            // Acknowledge successful processing
            if let Err(e) = ackable_message.ack().await {
                eprintln!("Failed to acknowledge task {}: {}", task.task_id, e);
            } else {
                println!("Task {} completed successfully", task.task_id);
            }
        }
    });

    // Give consumer time to start
    sleep(Duration::from_secs(1)).await;

    // Publish some tasks
    let tasks = vec![
        TaskMessage::new(1, "email", "Send welcome email"),
        TaskMessage::new(2, "report", "Generate daily report"),
        TaskMessage::new(3, "cleanup", "Clean up temp files"),
    ];

    for task in tasks {
        let routing_key = format!("tasks.{}", task.task_type);
        publisher.publish(&task, routing_key).await?;
        println!("Published task: {}", task.task_id);
        
        // Small delay between publications
        sleep(Duration::from_millis(100)).await;
    }

    // Wait for consumer to process messages
    tokio::select! {
        _ = consumer_handle => {
            println!("Consumer finished");
        }
        _ = sleep(Duration::from_secs(10)) => {
            println!("Example completed");
        }
    }

    Ok(())
}
```

#### Add to Cargo.toml

```toml
[dependencies]
rabbitmq_streamer = "0.2.0"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1.0"
```


