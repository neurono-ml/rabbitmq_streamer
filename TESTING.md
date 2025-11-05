# Testing

This project includes comprehensive tests using testcontainers to ensure that both `RabbitPublisher` and `RabbitConsumer` work correctly with a real RabbitMQ instance.

## Test Structure

### Publisher Tests (`tests/publisher_tests.rs`)
- **`test_publisher_connect`**: Verifies that the publisher can connect to RabbitMQ
- **`test_publisher_publish_message`**: Tests publishing a single message
- **`test_publisher_publish_multiple_messages`**: Tests publishing multiple messages
- **`test_publisher_with_different_routing_keys`**: Tests publishing with different routing keys

### Consumer Tests (`tests/consumer_tests.rs`)
- **`test_consumer_connect`**: Verifies that the consumer can connect to RabbitMQ
- **`test_consumer_load_messages`**: Tests consuming messages with automatic acknowledgment
- **`test_consumer_load_ackable_messages`**: Tests consuming messages with manual acknowledgment
- **`test_consumer_with_custom_tag`**: Tests using custom tags for consumers

### Integration Tests (`tests/integration_tests.rs`)
- **`test_publisher_consumer_integration`**: Tests complete integration between publisher and consumer
- **`test_publisher_consumer_ackable_integration`**: Tests integration with messages requiring manual acknowledgment
- **`test_multiple_routing_keys_integration`**: Tests scenarios with multiple routing keys

## Running the Tests

### Prerequisites
- Docker must be installed and running (testcontainers uses Docker to create RabbitMQ instances)
- Rust and Cargo installed

### Commands

```bash
# Run all tests
cargo test

# Run only publisher tests
cargo test --test publisher_tests

# Run only consumer tests
cargo test --test consumer_tests

# Run only integration tests
cargo test --test integration_tests

# Run tests with detailed output
cargo test -- --nocapture
```

## Technologies Used

- **testcontainers**: To create isolated RabbitMQ instances in Docker containers
- **testcontainers-modules**: Specific module for RabbitMQ
- **tokio-test**: Utilities for asynchronous tests
- **serde**: For serialization/deserialization of test messages

## Test Structures

### TestMessage (publisher_tests.rs and consumer_tests.rs)
```rust
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct TestMessage {
    id: u32,
    content: String,
    timestamp: u64,
}
```

### OrderEvent (integration_tests.rs)
```rust
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct OrderEvent {
    order_id: u32,
    customer_id: u32,
    action: String,
    amount: f64,
    timestamp: u64,
}
```

## Test Configuration

The tests automatically create:
- Temporary RabbitMQ instances using Docker
- Durable exchanges with appropriate configuration
- Queues bound to exchanges with appropriate routing keys
- Test connections with `test:test` credentials

Each test is isolated and uses unique resources to avoid interference between tests.

## Troubleshooting

### Docker not found
Make sure Docker is installed and the daemon is running:
```bash
docker --version
sudo systemctl start docker
```

### Test timeouts
The tests include appropriate timeouts, but if you're on a slow machine, you may need to increase the timeout values in the test files.

### Docker permission issues
If you get Docker permission errors:
```bash
sudo usermod -aG docker $USER
# Log out and log back in
```