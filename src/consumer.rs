use std::{fmt::Debug, time::Duration};

use futures::StreamExt;
use lapin::{
    options::{BasicConsumeOptions, BasicNackOptions},
    types::FieldTable,
    Connection, ConnectionProperties, Consumer,
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
    uri: String,
    queue_name: String,
}

impl RabbitConsumer {
    /// Establishes a new RabbitConsumer by connecting to a RabbitMQ broker and binding a queue to an exchange.
    ///
    /// # Parameters
    /// * `uri` - The RabbitMQ connection URI.
    /// * `queue_name` - The name of the queue to consume from.
    ///
    /// # Returns
    /// A configured `RabbitConsumer`.
    #[instrument]
    pub async fn connect(uri: &str, queue_name: &str) -> anyhow::Result<Self> {
        Ok(RabbitConsumer {
            uri: uri.to_string(),
            queue_name: queue_name.to_string(),
        })
    }

    /// Loads messages from the queue and automatically acknowledges them upon receipt.
    ///
    /// # Parameters
    /// * `channel_capacity` - Internal channel buffer size.
    /// * `consumer_tag` - Identifier used for consumer instance.
    ///
    /// # Returns
    /// An `mpsc::Receiver<T>` for consuming deserialized messages.
    #[instrument]
    pub async fn load_messages<T, C>(
        &self,
        channel_capacity: usize,
        consumer_tag: Option<C>,
    ) -> anyhow::Result<Receiver<T>>
    where
        T: DeserializeOwned + Send + Sync + 'static,
        C: Into<String> + Debug,
    {
        let (sender, receiver) = mpsc::channel::<T>(channel_capacity);
        let uri = self.uri.clone();
        let queue = self.queue_name.clone();
        let tag = if let Some(consumer_tag_) = consumer_tag {
            consumer_tag_.into()
        } else {
            format!("{}", uuid::Uuid::new_v4())
        };

        tokio::spawn(async move {
            loop {
                match Self::create_consumer(&uri, &queue, &tag).await {
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
    /// * `consumer_tag` - Identifier used for consumer instance.
    ///
    /// # Returns
    /// An `mpsc::Receiver<AckableMessage<T>>` for consuming messages with manual acknowledgment.
    #[instrument]
    pub async fn load_ackable_messages<T, C>(
        &self,
        channel_capacity: usize,
        consumer_tag: Option<C>,
    ) -> anyhow::Result<Receiver<AckableMessage<T>>>
    where
        T: DeserializeOwned + Send + Sync + 'static,
        C: Into<String> + Debug,
    {
        let (sender, receiver) = mpsc::channel::<AckableMessage<T>>(channel_capacity);
        let uri = self.uri.clone();
        let queue = self.queue_name.clone();
        let tag = if let Some(consumer_tag_) = consumer_tag {
            consumer_tag_.into()
        } else {
            format!("{}", uuid::Uuid::new_v4())
        };

        tokio::spawn(async move {
            loop {
                match Self::create_consumer(&uri, &queue, &tag).await {
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
                                            let _ = delivery
                                                .nack(BasicNackOptions {
                                                    multiple: false,
                                                    requeue: false,
                                                })
                                                .await;
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

    /// Creates a new channel
    async fn create_consumer(
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
