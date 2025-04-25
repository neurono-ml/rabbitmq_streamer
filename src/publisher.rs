use lapin::{options::{BasicPublishOptions, ExchangeDeclareOptions}, types::FieldTable, BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind};
use serde::Serialize;
use std::sync::Arc;
use tracing::instrument;

use crate::common::make_routing_key;

/// A struct representing a RabbitMQ publisher. It holds the necessary information to publish messages to an exchange.
/// **Parameters**
/// * `channel`: A reference-counted handle to a channel, which is used to send messages.
/// * `exchange_name`: The name of the exchange where messages will be published.
/// * `app_group_namespace`: An identifier for the application used to create routing keys. This helps in segregating messages based on different applications or groups.
pub struct RabbitPublisher {
    channel: Arc<Channel>,
    exchange_name: String,
    app_group_namespace: String,
}

impl RabbitPublisher {
    /// Connects to a RabbitMQ broker and binds the specified exchange. Use app_group_namespace
    /// to identify your application while creating a routing key.
    /// **Parameters**
    /// * `uri`: The RabbitMQ broker URI.
    /// * `exchange`: The name of the exchange to bind.
    /// * `app_group_namespace`: An identifier for the application used to create routig keys.
    /// **Returns**
    /// * `RabbitPublisher` - An instance of the publisher connected to the specified RabbitMQ 
    #[instrument]
    pub async fn connect(uri: &str, exchange: &str, app_group_namespace: &str) -> anyhow::Result<Self> {
        let connection_properties = ConnectionProperties::default();
        let connection = Connection::connect(uri, connection_properties).await?;

        let mut channel = connection.create_channel().await?;
        let channel = bind_exchange(&mut channel, exchange).await?;

        let publisher = Self {
            channel: Arc::new(channel),
            exchange_name: exchange.to_owned(),
            app_group_namespace: app_group_namespace.to_owned(),
        };

        Ok(publisher)
    }

    /// Publishes a message to the specified destination queue.
    /// **Parameters**
    /// * `destination_queue`: The name of the destination queue.
    /// * `message`: The message to be published.
    /// * `payload`: The message payload.
    /// **Returns**
    /// * `anyhow::Result<()>` - Returns an empty result if the message was successfully published.
    #[instrument(skip(self, message))]
    pub async fn publish<T: Serialize>(
        &self,
        destination_queue: &str,
        message: &T,
    ) -> anyhow::Result<()> {
        let routing_key = make_routing_key(&self.app_group_namespace, destination_queue);
        let payload = serde_json::to_string(&message)?;
        let publish_options = BasicPublishOptions{mandatory: true, immediate: false };
        
        let properties = BasicProperties::default();

        self.channel
            .basic_publish(&self.exchange_name, &routing_key, publish_options, payload.as_bytes(), properties)
            .await?;
        Ok(())
    }
}


async fn bind_exchange(channel: &mut Channel, exchange_name: &str) -> anyhow::Result<Channel> {
    let exchange_declare_options = ExchangeDeclareOptions {
        durable: true,
        auto_delete: false,
        internal: false,
        nowait: false,
        passive: false,
        ..ExchangeDeclareOptions::default()
    };
    
    channel.exchange_declare(exchange_name, ExchangeKind::Direct, exchange_declare_options, FieldTable::default()).await?;

    Ok(channel.to_owned())
}

#[cfg(test)]
mod tests {
    use lapin::options::{ExchangeDeleteOptions, QueueBindOptions, QueueDeclareOptions, QueueDeleteOptions};

    use super::*;

    use crate::common::make_routing_key;

    const AMQP_URL: &str = "amqp://admin:password@pipe:5672";
    const EXCHANGE_NAME: &str = "test";
    const QUEUE_NAME: &str = "incomming_addresses";
    const APP_GROUP_NAMESPACE: &str = "test_grup";
    

    async fn setup_queue(exchange_name: &str) -> anyhow::Result<Channel> {
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

        let queue_declare_options = QueueDeclareOptions{
            passive: true,
            durable: true,
            exclusive: false,
            auto_delete: false,
            nowait: false,
        };
        
        let routing_key = make_routing_key(APP_GROUP_NAMESPACE, QUEUE_NAME);
        let queue_bind_options = QueueBindOptions { nowait: false, };
        channel.queue_declare(EXCHANGE_NAME, queue_declare_options, FieldTable::default()).await?;
        channel.queue_bind(QUEUE_NAME, EXCHANGE_NAME, &routing_key, queue_bind_options, FieldTable::default()).await?;

        Ok(channel)
    }

    async fn setup_publisher() -> RabbitPublisher {
        let publisher_client = RabbitPublisher::connect(AMQP_URL, EXCHANGE_NAME, APP_GROUP_NAMESPACE).await.unwrap();
        publisher_client
    }

    async fn tear_down(channel: Channel) -> anyhow::Result<()> {

        channel.exchange_delete(QUEUE_NAME, ExchangeDeleteOptions::default()).await?;
        channel.queue_delete(QUEUE_NAME, QueueDeleteOptions::default()).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_one_message_can_be_published() -> anyhow::Result<()> {
        let channel = setup_queue(EXCHANGE_NAME).await?;
        let client = setup_publisher().await;

        let addresses = vec![
            "Rua Barão de Mesquita".to_owned(),
            "Rua Zé das couves".to_owned(),
            "Rua Perto da Padaria".to_owned(),
        ];

        tear_down(channel).await?;
        Ok(())        
    }
}