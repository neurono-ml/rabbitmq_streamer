use rabbitmq_streamer::RabbitConsumer;
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

async fn setup_queue_with_messages(
    connection_string: &str,
    queue_name: &str,
) -> anyhow::Result<()> {
    use lapin::{
        options::{
            BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
        },
        types::FieldTable,
        BasicProperties, Connection, ConnectionProperties, ExchangeKind,
    };

    let connection =
        Connection::connect(connection_string, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;

    // Declare exchange with durable=true to match publisher configuration
    channel
        .exchange_declare(
            "test_exchange",
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                auto_delete: false,
                internal: false,
                nowait: false,
                passive: false,
            },
            FieldTable::default(),
        )
        .await?;

    // Declare queue
    channel
        .queue_declare(
            queue_name,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Bind queue to exchange
    channel
        .queue_bind(
            queue_name,
            "test_exchange",
            "test.routing.key",
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    // Publish some test messages
    for i in 1..=3 {
        let message = TestMessage::new(i, &format!("Test message {}", i));
        let payload = serde_json::to_string(&message)?;
        channel
            .basic_publish(
                "test_exchange",
                "test.routing.key",
                BasicPublishOptions::default(),
                payload.as_bytes(),
                BasicProperties::default(),
            )
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_consumer_connect() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let _consumer = RabbitConsumer::connect(&connection_string, "test_queue")
        .await
        .expect("Failed to connect consumer");

    // If we get here without panicking, the connection was successful
}

#[tokio::test]
async fn test_consumer_load_messages() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    // Setup queue with messages
    setup_queue_with_messages(&connection_string, "test_queue_load")
        .await
        .expect("Failed to setup queue with messages");

    let consumer = RabbitConsumer::connect(&connection_string, "test_queue_load")
        .await
        .expect("Failed to connect consumer");

    let mut message_receiver = consumer
        .load_messages::<TestMessage, _>(10, Some("test-consumer"))
        .await
        .expect("Failed to load messages");

    // Give some time for messages to be consumed
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let mut received_messages = Vec::new();
    let mut count = 0;

    // Try to receive messages with timeout
    while count < 3 {
        tokio::select! {
            message = message_receiver.recv() => {
                if let Some(msg) = message {
                    received_messages.push(msg);
                    count += 1;
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                break;
            }
        }
    }

    assert!(
        !received_messages.is_empty(),
        "Should have received at least one message"
    );
    assert!(
        received_messages.len() <= 3,
        "Should not receive more than 3 messages"
    );

    // Verify message structure
    for msg in &received_messages {
        assert!(msg.id > 0);
        assert!(msg.content.contains("Test message"));
    }
}

#[tokio::test]
async fn test_consumer_load_ackable_messages() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    // Setup queue with messages
    setup_queue_with_messages(&connection_string, "test_queue_ackable")
        .await
        .expect("Failed to setup queue with messages");

    let consumer = RabbitConsumer::connect(&connection_string, "test_queue_ackable")
        .await
        .expect("Failed to connect consumer");

    let mut message_receiver = consumer
        .load_ackable_messages::<TestMessage, _>(10, Some("test-ackable-consumer"))
        .await
        .expect("Failed to load ackable messages");

    // Give some time for messages to be consumed
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let mut received_messages = Vec::new();
    let mut count = 0;

    // Try to receive messages with timeout
    while count < 3 {
        tokio::select! {
            message = message_receiver.recv() => {
                if let Some(ackable_msg) = message {
                    let msg = ackable_msg.message();
                    received_messages.push(msg);

                    // Acknowledge the message
                    ackable_msg.ack().await.expect("Failed to ack message");
                    count += 1;
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                break;
            }
        }
    }

    assert!(
        !received_messages.is_empty(),
        "Should have received at least one ackable message"
    );
    assert!(
        received_messages.len() <= 3,
        "Should not receive more than 3 ackable messages"
    );

    // Verify message structure
    for msg in &received_messages {
        assert!(msg.id > 0);
        assert!(msg.content.contains("Test message"));
    }
}

#[tokio::test]
async fn test_consumer_with_custom_tag() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    // Setup queue with messages
    setup_queue_with_messages(&connection_string, "test_queue_custom_tag")
        .await
        .expect("Failed to setup queue with messages");

    let consumer = RabbitConsumer::connect(&connection_string, "test_queue_custom_tag")
        .await
        .expect("Failed to connect consumer");

    let custom_tag = "my-custom-consumer-tag";
    let mut message_receiver = consumer
        .load_messages::<TestMessage, _>(5, Some(custom_tag))
        .await
        .expect("Failed to load messages with custom tag");

    // Give some time for messages to be consumed
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Just verify we can receive at least one message
    tokio::select! {
        message = message_receiver.recv() => {
            assert!(message.is_some(), "Should have received a message with custom tag");
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {
            panic!("Timeout waiting for message with custom tag");
        }
    }
}
