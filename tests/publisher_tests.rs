use rabbitmq_streamer::RabbitPublisher;
use serde::{Deserialize, Serialize};
use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
use testcontainers_modules::rabbitmq::RabbitMq;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct TestMessage {
    id: u32,
    content: String,
    timestamp: u64,
}

impl TestMessage {
    fn new(id: u32, content: &str) -> Self {
        Self {
            id,
            content: content.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

async fn setup_rabbitmq_container() -> (ContainerAsync<RabbitMq>, String) {
    let rabbitmq_image = RabbitMq::default()
        .with_env_var("RABBITMQ_DEFAULT_USER", "test")
        .with_env_var("RABBITMQ_DEFAULT_PASS", "test");

    let container = rabbitmq_image
        .start()
        .await
        .expect("Failed to start RabbitMQ container");

    let host = container
        .get_host()
        .await
        .expect("Failed to get container host");
    let port = container
        .get_host_port_ipv4(5672)
        .await
        .expect("Failed to get container port");
    let connection_string = format!("amqp://test:test@{}:{}", host, port);

    // Wait a bit for RabbitMQ to be fully ready
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    (container, connection_string)
}

#[tokio::test]
async fn test_publisher_connect() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let _publisher = RabbitPublisher::connect(&connection_string, "test_exchange")
        .await
        .expect("Failed to connect publisher");

    // If we get here without panicking, the connection was successful
    assert!(true);
}

#[tokio::test]
async fn test_publisher_publish_message() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let publisher = RabbitPublisher::connect(&connection_string, "test_exchange")
        .await
        .expect("Failed to connect publisher");

    let test_message = TestMessage::new(1, "Hello RabbitMQ!");

    let result = publisher.publish(&test_message, "test.routing.key").await;

    assert!(result.is_ok(), "Failed to publish message: {:?}", result);
}

#[tokio::test]
async fn test_publisher_publish_multiple_messages() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let publisher = RabbitPublisher::connect(&connection_string, "test_exchange")
        .await
        .expect("Failed to connect publisher");

    for i in 1..=5 {
        let test_message = TestMessage::new(i, &format!("Message {}", i));
        let result = publisher
            .publish(&test_message, format!("test.routing.key.{}", i))
            .await;
        assert!(
            result.is_ok(),
            "Failed to publish message {}: {:?}",
            i,
            result
        );
    }
}

#[tokio::test]
async fn test_publisher_with_different_routing_keys() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let publisher = RabbitPublisher::connect(&connection_string, "test_exchange")
        .await
        .expect("Failed to connect publisher");

    let routing_keys = vec!["orders.created", "orders.updated", "orders.deleted"];

    for (i, routing_key) in routing_keys.iter().enumerate() {
        let test_message = TestMessage::new(i as u32 + 1, &format!("Order event: {}", routing_key));
        let result = publisher.publish(&test_message, *routing_key).await;
        assert!(
            result.is_ok(),
            "Failed to publish message with routing key {}: {:?}",
            routing_key,
            result
        );
    }
}
