use rabbitmq_streamer::{RabbitConsumer, RabbitPublisher};
use serde::{Deserialize, Serialize};
use testcontainers::{runners::AsyncRunner, ContainerAsync, ImageExt};
use testcontainers_modules::rabbitmq::RabbitMq;
use tokio::time::{timeout, Duration};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct OrderEvent {
    order_id: u32,
    customer_id: u32,
    action: String,
    amount: f64,
    timestamp: u64,
}

impl OrderEvent {
    fn new(order_id: u32, customer_id: u32, action: &str, amount: f64) -> Self {
        Self {
            order_id,
            customer_id,
            action: action.to_string(),
            amount,
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
    tokio::time::sleep(Duration::from_secs(3)).await;

    (container, connection_string)
}

async fn setup_queue_binding(
    connection_string: &str,
    queue_name: &str,
    exchange_name: &str,
    routing_key: &str,
) -> anyhow::Result<()> {
    use lapin::{
        options::{ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
        types::FieldTable,
        Connection, ConnectionProperties, ExchangeKind,
    };

    let connection =
        Connection::connect(connection_string, ConnectionProperties::default()).await?;
    let channel = connection.create_channel().await?;

    // Declare exchange with durable=true to match publisher configuration
    channel
        .exchange_declare(
            exchange_name,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                auto_delete: false,
                internal: false,
                nowait: false,
                passive: false,
                ..ExchangeDeclareOptions::default()
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
            exchange_name,
            routing_key,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_publisher_consumer_integration() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let exchange_name = "orders_exchange";
    let queue_name = "orders_queue";
    let routing_key = "orders.created";

    // Setup queue binding
    setup_queue_binding(&connection_string, queue_name, exchange_name, routing_key)
        .await
        .expect("Failed to setup queue binding");

    // Create publisher
    let publisher = RabbitPublisher::connect(&connection_string, exchange_name, "test_app")
        .await
        .expect("Failed to connect publisher");

    // Create consumer
    let consumer = RabbitConsumer::connect(&connection_string, queue_name)
        .await
        .expect("Failed to connect consumer");

    // Start consuming messages
    let mut message_receiver = consumer
        .load_messages::<OrderEvent, _>(10, Some("integration-test-consumer"))
        .await
        .expect("Failed to start consuming messages");

    // Give consumer a moment to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish test messages
    let test_orders = vec![
        OrderEvent::new(1, 100, "created", 99.99),
        OrderEvent::new(2, 101, "created", 149.50),
        OrderEvent::new(3, 102, "created", 299.00),
    ];

    for order in &test_orders {
        publisher
            .publish(order, routing_key)
            .await
            .expect("Failed to publish order event");
    }

    // Collect received messages
    let mut received_orders = Vec::new();
    let receive_timeout = Duration::from_secs(5);

    for _ in 0..test_orders.len() {
        match timeout(receive_timeout, message_receiver.recv()).await {
            Ok(Some(order)) => {
                received_orders.push(order);
            }
            Ok(None) => {
                break; // Channel closed
            }
            Err(_) => {
                break; // Timeout
            }
        }
    }

    // Verify we received the messages
    assert_eq!(
        received_orders.len(),
        test_orders.len(),
        "Should have received all {} published messages, got {}",
        test_orders.len(),
        received_orders.len()
    );

    // Verify message content (order may vary)
    for received_order in &received_orders {
        let matching_order = test_orders.iter().find(|&test_order| {
            test_order.order_id == received_order.order_id
                && test_order.customer_id == received_order.customer_id
                && test_order.action == received_order.action
                && (test_order.amount - received_order.amount).abs() < 0.01
        });

        assert!(
            matching_order.is_some(),
            "Received order {:?} should match one of the published orders",
            received_order
        );
    }
}

#[tokio::test]
async fn test_publisher_consumer_ackable_integration() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let exchange_name = "orders_ackable_exchange";
    let queue_name = "orders_ackable_queue";
    let routing_key = "orders.updated";

    // Setup queue binding
    setup_queue_binding(&connection_string, queue_name, exchange_name, routing_key)
        .await
        .expect("Failed to setup queue binding");

    // Create publisher
    let publisher = RabbitPublisher::connect(&connection_string, exchange_name, "test_app")
        .await
        .expect("Failed to connect publisher");

    // Create consumer
    let consumer = RabbitConsumer::connect(&connection_string, queue_name)
        .await
        .expect("Failed to connect consumer");

    // Start consuming ackable messages
    let mut message_receiver = consumer
        .load_ackable_messages::<OrderEvent, _>(10, Some("ackable-integration-test-consumer"))
        .await
        .expect("Failed to start consuming ackable messages");

    // Give consumer a moment to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish test messages
    let test_orders = vec![
        OrderEvent::new(10, 200, "updated", 109.99),
        OrderEvent::new(11, 201, "updated", 159.50),
    ];

    for order in &test_orders {
        publisher
            .publish(order, routing_key)
            .await
            .expect("Failed to publish order event");
    }

    // Collect and acknowledge received messages
    let mut received_orders = Vec::new();
    let receive_timeout = Duration::from_secs(5);

    for _ in 0..test_orders.len() {
        match timeout(receive_timeout, message_receiver.recv()).await {
            Ok(Some(ackable_message)) => {
                let order = ackable_message.message();
                received_orders.push(order);

                // Acknowledge the message
                ackable_message
                    .ack()
                    .await
                    .expect("Failed to acknowledge message");
            }
            Ok(None) => {
                break; // Channel closed
            }
            Err(_) => {
                break; // Timeout
            }
        }
    }

    // Verify we received and acknowledged the messages
    assert_eq!(
        received_orders.len(),
        test_orders.len(),
        "Should have received all {} published ackable messages, got {}",
        test_orders.len(),
        received_orders.len()
    );

    // Verify message content
    for received_order in &received_orders {
        let matching_order = test_orders.iter().find(|&test_order| {
            test_order.order_id == received_order.order_id
                && test_order.action == received_order.action
        });

        assert!(
            matching_order.is_some(),
            "Received ackable order {:?} should match one of the published orders",
            received_order
        );
    }
}

#[tokio::test]
async fn test_multiple_routing_keys_integration() {
    let (_container, connection_string) = setup_rabbitmq_container().await;

    let exchange_name = "multi_routing_exchange";
    let queue_name = "multi_routing_queue";

    // Setup multiple routing key bindings
    let routing_keys = vec!["orders.created", "orders.updated", "orders.deleted"];

    for routing_key in &routing_keys {
        setup_queue_binding(&connection_string, queue_name, exchange_name, routing_key)
            .await
            .expect(&format!(
                "Failed to setup queue binding for {}",
                routing_key
            ));
    }

    // Create publisher
    let publisher = RabbitPublisher::connect(&connection_string, exchange_name, "test_app")
        .await
        .expect("Failed to connect publisher");

    // Create consumer
    let consumer = RabbitConsumer::connect(&connection_string, queue_name)
        .await
        .expect("Failed to connect consumer");

    // Start consuming messages
    let mut message_receiver = consumer
        .load_messages::<OrderEvent, _>(20, Some("multi-routing-consumer"))
        .await
        .expect("Failed to start consuming messages");

    // Give consumer a moment to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish messages with different routing keys
    let test_data = vec![
        (
            OrderEvent::new(20, 300, "created", 199.99),
            "orders.created",
        ),
        (
            OrderEvent::new(21, 301, "updated", 219.99),
            "orders.updated",
        ),
        (OrderEvent::new(22, 302, "deleted", 0.0), "orders.deleted"),
        (OrderEvent::new(23, 303, "created", 99.99), "orders.created"),
    ];

    for (order, routing_key) in &test_data {
        publisher
            .publish(order, *routing_key)
            .await
            .expect(&format!(
                "Failed to publish order with routing key {}",
                routing_key
            ));
    }

    // Collect received messages
    let mut received_orders = Vec::new();
    let receive_timeout = Duration::from_secs(5);

    for _ in 0..test_data.len() {
        match timeout(receive_timeout, message_receiver.recv()).await {
            Ok(Some(order)) => {
                received_orders.push(order);
            }
            Ok(None) => {
                break; // Channel closed
            }
            Err(_) => {
                break; // Timeout
            }
        }
    }

    // Verify we received all messages from different routing keys
    assert_eq!(
        received_orders.len(),
        test_data.len(),
        "Should have received all {} messages from different routing keys, got {}",
        test_data.len(),
        received_orders.len()
    );

    // Count messages by action type
    let created_count = received_orders
        .iter()
        .filter(|o| o.action == "created")
        .count();
    let updated_count = received_orders
        .iter()
        .filter(|o| o.action == "updated")
        .count();
    let deleted_count = received_orders
        .iter()
        .filter(|o| o.action == "deleted")
        .count();

    assert_eq!(
        created_count, 2,
        "Should have received 2 'created' messages"
    );
    assert_eq!(updated_count, 1, "Should have received 1 'updated' message");
    assert_eq!(deleted_count, 1, "Should have received 1 'deleted' message");
}
