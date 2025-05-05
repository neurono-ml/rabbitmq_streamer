use std::time::Duration;

use futures::StreamExt;
use lapin::{
    options::{BasicConsumeOptions, BasicNackOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable,
    Channel, Connection, ConnectionProperties, Consumer,
};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::{self, Receiver};
use tokio::time::sleep;
use tracing::instrument;

use crate::{common::make_routing_key, rabbit_message::AckableMessage};

/// A struct representing a RabbitMQ consumer.
/// It manages connection, channel, and message consumption from a specified queue.
#[derive(Debug)]
pub struct RabbitConsumer {
    uri: String,
    exchange_name: String,
    queue_name: String,
    app_group_namespace: String,
}

impl RabbitConsumer {
    /// Establishes a new RabbitConsumer by connecting to a RabbitMQ broker and binding a queue to an exchange.
    ///
    /// # Parameters
    /// * `uri` - The RabbitMQ connection URI.
    /// * `queue_name` - The name of the queue to consume from.
    /// * `app_group_namespace` - A namespace for logical separation of consumers.
    /// * `exchange_name` - The name of the exchange to bind to.
    ///
    /// # Returns
    /// A configured `RabbitConsumer`.
    #[instrument]
    pub async fn connect(
        uri: &str,
        queue_name: &str,
        app_group_namespace: &str,
        exchange_name: &str,
    ) -> anyhow::Result<Self> {
        Ok(RabbitConsumer {
            uri: uri.to_string(),
            queue_name: queue_name.to_string(),
            app_group_namespace: app_group_namespace.to_string(),
            exchange_name: exchange_name.to_string(),
        })
    }

    /// Loads messages from the queue and automatically acknowledges them upon receipt.
    ///
    /// # Parameters
    /// * `channel_capacity` - Internal channel buffer size.
    /// * `tag` - Identifier used for consumer instance.
    ///
    /// # Returns
    /// An `mpsc::Receiver<T>` for consuming deserialized messages.
    #[instrument]
    pub async fn load_messages<T>(
        &self,
        channel_capacity: usize,
        tag: &str,
    ) -> anyhow::Result<Receiver<T>>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        let (sender, receiver) = mpsc::channel::<T>(channel_capacity);
        let consumer_tag = format!("{}-{}", self.app_group_namespace, tag);
        let uri = self.uri.clone();
        let exchange = self.exchange_name.clone();
        let queue = self.queue_name.clone();
        let namespace = self.app_group_namespace.clone();

        tokio::spawn(async move {
            loop {
                match Self::create_channel_and_consumer(&uri, &queue, &exchange, &namespace, &consumer_tag).await {
                    Ok(mut consumer) => {
                        log::info!("Consumer started.");
                        while let Some(result) = consumer.next().await {
                            match result {
                                Ok(delivery) => {
                                    let message: T = match serde_json::from_slice(&delivery.data) {
                                        Ok(m) => m,
                                        Err(e) => {
                                            log::error!("Failed to deserialize message: {:?}", e);
                                            continue;
                                        }
                                    };
                                    if sender.send(message).await.is_err() {
                                        log::error!("Receiver dropped, stopping.");
                                        return;
                                    }
                                }
                                Err(e) => {
                                    log::error!("Consumer error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        log::warn!("Consumer stream ended, retrying in 5s...");
                    }
                    Err(e) => {
                        log::error!("Failed to create consumer: {:?}", e);
                    }
                }
                sleep(Duration::from_secs(5)).await;
            }
        });

        Ok(receiver)
    }

    /// Loads messages from the queue that require manual acknowledgement.
    ///
    /// # Parameters
    /// * `channel_capacity` - Internal channel buffer size.
    /// * `tag` - Identifier used for consumer instance.
    ///
    /// # Returns
    /// An `mpsc::Receiver<AckableMessage<T>>` for consuming messages with manual acknowledgment.
    #[instrument]
    pub async fn load_ackable_messages<T>(
        &self,
        channel_capacity: usize,
        tag: &str,
    ) -> anyhow::Result<Receiver<AckableMessage<T>>>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        let (sender, receiver) = mpsc::channel::<AckableMessage<T>>(channel_capacity);
        let consumer_tag = format!("{}-{}", self.app_group_namespace, tag);
        let uri = self.uri.clone();
        let exchange = self.exchange_name.clone();
        let queue = self.queue_name.clone();
        let namespace = self.app_group_namespace.clone();

        tokio::spawn(async move {
            loop {
                match Self::create_channel_and_consumer(&uri, &queue, &exchange, &namespace, &consumer_tag).await {
                    Ok(mut consumer) => {
                        log::info!("Ackable consumer started.");
                        while let Some(result) = consumer.next().await {
                            match result {
                                Ok(delivery) => {
                                    let parsed = serde_json::from_slice::<T>(&delivery.data);
                                    match parsed {
                                        Ok(message) => {
                                            let ackable = AckableMessage::new(message, delivery);
                                            if sender.send(ackable).await.is_err() {
                                                log::error!("Ackable receiver dropped, stopping.");
                                                return;
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Failed to deserialize: {:?}", e);
                                            let _ = delivery.nack(BasicNackOptions {
                                                multiple: false,
                                                requeue: false,
                                            }).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    log::error!("Consumer error: {:?}", e);
                                    break;
                                }
                            }
                        }
                        log::warn!("Ackable consumer stream ended, retrying in 5s...");
                    }
                    Err(e) => {
                        log::error!("Failed to create ackable consumer: {:?}", e);
                    }
                }
                sleep(Duration::from_secs(5)).await;
            }
        });

        Ok(receiver)
    }

    /// Creates a new channel and binds a consumer to the queue.
    async fn create_channel_and_consumer(
        uri: &str,
        queue_name: &str,
        exchange_name: &str,
        namespace: &str,
        consumer_tag: &str,
    ) -> anyhow::Result<Consumer> {
        let conn = Connection::connect(uri, ConnectionProperties::default()).await?;
        let mut channel = conn.create_channel().await?;
        let routing_key = make_routing_key(namespace, queue_name);
        bind_queue(&mut channel, queue_name, exchange_name, &routing_key).await?;

        let consumer = channel
            .basic_consume(
                queue_name,
                consumer_tag,
                BasicConsumeOptions {
                    no_ack: false,
                    nowait: false,
                    exclusive: false,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        Ok(consumer)
    }
}

/// Declares and binds a queue to an exchange with a routing key.
///
/// # Returns
/// The channel after queue binding.
async fn bind_queue(
    channel: &mut Channel,
    queue_name: &str,
    exchange_name: &str,
    routing_key: &str,
) -> anyhow::Result<Channel> {
    let queue_opts = QueueDeclareOptions {
        durable: true,
        ..Default::default()
    };

    channel.queue_declare(queue_name, queue_opts, FieldTable::default()).await?;
    channel.queue_bind(queue_name, exchange_name, routing_key, QueueBindOptions::default(), FieldTable::default()).await?;

    Ok(channel.to_owned())
}
