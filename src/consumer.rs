use std::sync::Arc;

use futures::StreamExt;
use lapin::{options::{BasicConsumeOptions, QueueBindOptions, QueueDeclareOptions},
    types::FieldTable, Channel, Connection, ConnectionProperties};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::Receiver;
use tracing::instrument;

use crate::{common::make_routing_key, rabbit_message::AckableMessage};

/// A struct representing a RabbitMQ consumer. It holds the necessary information to consume messages from a queue.
/// **Parameters**
/// * `channel`: A reference-counted handle to a channel, which is used to receive messages.
/// * `queue_name`: The name of the queue where messages will be consumed.
/// * `app_group_namespace`: An identifier for the application used to create routing keys. This helps in segregating messages based on different applications or groups.
#[derive(Debug)]
pub struct RabbitConsumer {
    channel: Arc<Channel>,
    queue_name: String,
    app_group_namespace: String,
}

impl RabbitConsumer {
    /// Connects to a RabbitMQ broker and binds the specified queue to an exchange.
    /// **Parameters**
    /// * `uri`: The RabbitMQ broker URI.
    /// * `queue_name`: The name of the queue to bind.
    /// * `app_group_namespace`: An identifier for the application used to create routing keys.
    /// **Returns**
    /// * `RabbitConsumer` - An instance of the consumer connected to the specified RabbitMQ broker.
    /// * `exchange_name`: The name of the exchange to bind.
    /// **Returns**
    /// * `RabbitConsumer` - An instance of the consumer connected to the specified RabbitMQ broker.
    #[instrument]
    pub async fn connect(uri: &str, queue_name: &str, app_group_namespace: &str, exchange_name: &str) -> anyhow::Result<Self> {
        let connection_properties = ConnectionProperties::default();
        let connection = Connection::connect(uri, connection_properties).await?;

        let mut channel = connection.create_channel().await?;

        let routing_key = make_routing_key(app_group_namespace, queue_name);
        let channel = bind_queue(&mut channel, queue_name, exchange_name, &routing_key).await?;

        let consumer = RabbitConsumer{
            channel: Arc::new(channel),
            queue_name: queue_name.to_owned(),
            app_group_namespace: app_group_namespace.to_owned(),
        };

        Ok(consumer)
    }

    /// Loads messages from the specified queue, they are automatically acked when they are received.
    /// **Parameters**
    /// * `channel_capacity`: The capacity of the internal channel to use for exchanging messages.
    /// * `tag`: A unique identifier for the consumer.
    /// **Returns**
    /// * `tokio::sync::mpsc::Receiver<T>` - A receiver that can be used to receive messages from the queue.
    #[instrument]
    pub async fn load_messages<T>(&self, channel_capacity: usize, tag: &str) -> anyhow::Result<Receiver<T>> where T: DeserializeOwned + Send + Sync + 'static {
        let consume_options = BasicConsumeOptions {
            no_ack: false,
            ..BasicConsumeOptions::default()
        };
        let consumer_tag = format!("{}-{}", self.app_group_namespace, tag);
        let mut channel_consumer = self.channel.basic_consume(&self.queue_name, &consumer_tag, consume_options, FieldTable::default()).await?;

        let (sender, receiver) = tokio::sync::mpsc::channel::<T>(channel_capacity);

        tokio::spawn(async move {
            while let Some(delivery) = channel_consumer.next().await.transpose()? {
                let message_content = delivery.data;
                let message = serde_json::from_slice(&message_content)?;
                sender.send(message).await?;
            }

            anyhow::Ok(())
        });
        

        Ok(receiver)
    }

    /// Loads messages from the specified queue, they should be manually acknowledged by the consumer.
    /// **Parameters**
    /// * `channel_capacity`: The capacity of the internal channel to use for exchanging messages.
    /// * `tag`: A unique identifier for the consumer.
    /// **Returns**
    /// * `tokio::sync::mpsc::Receiver<AckableMessage<T>>` - A receiver that can be used to receive messages from the queue.
    pub async fn load_ackable_messages<T>(&self, channel_capacity: usize, tag: &str) -> anyhow::Result<Receiver<AckableMessage<T>>> where T: DeserializeOwned + Send + Sync + 'static {
        let consume_options = BasicConsumeOptions {
            no_ack: true,
            ..BasicConsumeOptions::default()
        };
        let consumer_tag = format!("{}-{}", self.app_group_namespace, tag);
        let mut channel_consumer = self.channel.basic_consume(&self.queue_name, &consumer_tag, consume_options, FieldTable::default()).await?;

        let (sender, receiver) = tokio::sync::mpsc::channel::<AckableMessage<T>>(channel_capacity);

        tokio::spawn(async move {
            while let Some(delivery) = channel_consumer.next().await.transpose()? {
                let message_content = delivery.data.clone();
                let message = serde_json::from_slice(&message_content)?;
                let ackable_message = AckableMessage::new(message, delivery);
                sender.send(ackable_message).await?;
            }

            anyhow::Ok(())
        });
        

        Ok(receiver)
    }

}

async fn bind_queue(channel: &mut Channel, queue_name: &str, exchange_name: &str, routing_key: &str) -> anyhow::Result<Channel> {
    let queue_declare_options = QueueDeclareOptions {
        passive: false,
        durable: true,
        exclusive: false,
        auto_delete: false,
        nowait: false
    };

    channel.queue_declare(queue_name, queue_declare_options, FieldTable::default()).await?;

    let queue_bind_options = QueueBindOptions { nowait: false };
    channel.queue_bind(queue_name, exchange_name, routing_key, queue_bind_options, FieldTable::default()).await?;
    
    Ok(channel.to_owned())
}

#[cfg(test)]
mod tests {
    use lapin::{options::{BasicPublishOptions, ExchangeDeclareOptions, ExchangeDeleteOptions,
        QueueDeleteOptions},
        protocol::basic::AMQPProperties, ExchangeKind};

    use crate::common::make_routing_key;

    use super::*;

    const AMQP_URL: &str = "amqp://admin:password@pipe:5672";
    const EXCHANGE_NAME: &str = "test";
    const QUEUE_NAME: &str = "incomming_addresses";
    const APP_GROUP_NAMESPACE: &str = "test_group";
    const CAPACITY: usize = 10;
    const TAG: &str = "test-tag";

    async fn setup_consumer() -> RabbitConsumer {
        let query_queue_client = RabbitConsumer::connect(AMQP_URL, QUEUE_NAME, APP_GROUP_NAMESPACE, EXCHANGE_NAME).await.unwrap();
        query_queue_client
    }

    async fn setup_exchange() -> anyhow::Result<Channel> {
        let connection_properties = ConnectionProperties::default();
        let connection = Connection::connect(AMQP_URL, connection_properties).await?;
        let channel = connection.create_channel().await?;
        
        let exchange_declare_options = ExchangeDeclareOptions {
            durable: true,
            auto_delete: false,
            internal: false,
            nowait: false,
            passive: false,
            ..ExchangeDeclareOptions::default()
        };
        
        channel.exchange_declare(EXCHANGE_NAME, ExchangeKind::Direct, exchange_declare_options, FieldTable::default()).await?;

        Ok(channel)
    }

    async fn tear_down(channel: Channel) -> anyhow::Result<()> {

        channel.exchange_delete(EXCHANGE_NAME, ExchangeDeleteOptions::default()).await?;
        channel.queue_delete(QUEUE_NAME, QueueDeleteOptions::default()).await?;

        Ok(())
    }


    #[tokio::test]
    async fn test_one_message_can_be_loaded() -> anyhow::Result<()> {
        let addresses = vec![
            "Rua Barão de Mesquita".to_owned(),
            "Rua Zé das couves".to_owned(),
            "Rua Perto da Padaria".to_owned(),
        ];

        let routing_key = make_routing_key(APP_GROUP_NAMESPACE, QUEUE_NAME);
        let payload = serde_json::to_string(&addresses)?;
        
        let channel = setup_exchange().await?;
        let client = setup_consumer().await;

        let publish_options = BasicPublishOptions{mandatory: true, immediate: false};
        channel.basic_publish(EXCHANGE_NAME, &routing_key, publish_options, payload.as_bytes(), AMQPProperties::default()).await?;

        let mut receiver = client.load_messages::<Vec<String>>(CAPACITY, TAG).await?;

        let mut count = 0;
        while let Some(message) = receiver.recv().await {
            assert_eq!(message, addresses);
            count += 1;
            break
        }

        assert_eq!(count, 1);

        tear_down(channel).await?;

        Ok(())
    }
}