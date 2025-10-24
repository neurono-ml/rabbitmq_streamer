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

use crate::rabbit_message::AckableMessage;

/// A struct representing a RabbitMQ consumer.
/// It manages connection, channel, and message consumption from a specified queue.
#[derive(Debug)]
pub struct RabbitConsumer {
    pub(super) uri: String,
    pub(super) queue_name: String,
    pub(super) consumer_tag: String,
}

impl RabbitConsumer {
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
        let uri = self.uri.clone();
        let queue = self.queue_name.clone();
        let consumer_tag = self.consumer_tag.clone();

        tokio::spawn(async move {
            loop {
                match Self::create_channel_and_consumer(&uri, &queue, &consumer_tag).await {
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
        let uri = self.uri.clone();
        let queue = self.queue_name.clone();
        let consumer_tag = self.consumer_tag.clone();

        tokio::spawn(async move {
            loop {
                match Self::create_channel_and_consumer(&uri, &queue, &consumer_tag).await {
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
        consumer_tag: &str,
    ) -> anyhow::Result<Consumer> {
        let conn = Connection::connect(uri, ConnectionProperties::default()).await?;
        let channel = conn.create_channel().await?;

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
