use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::Serialize;
use std::{fmt::Debug, sync::Arc};
use tracing::instrument;

/// A struct representing a RabbitMQ publisher. It holds the necessary information to publish messages to an exchange.
/// **Parameters**
/// * `channel`: A reference-counted handle to a channel, which is used to send messages.
/// * `exchange_name`: The name of the exchange where messages will be published.
pub struct RabbitPublisher {
    channel: Arc<Channel>,
    exchange_name: String,
}

impl RabbitPublisher {
    /// Connects to a RabbitMQ broker and binds the specified exchange. Use app_group_namespace
    /// to identify your application while creating a routing key.
    /// **Parameters**
    /// * `uri`: The RabbitMQ broker URI.
    /// * `exchange`: The name of the exchange to bind.
    /// **Returns**
    /// * `RabbitPublisher` - An instance of the publisher connected to the specified RabbitMQ
    #[instrument]
    pub async fn connect(
        uri: &str,
        exchange: &str,
    ) -> anyhow::Result<Self> {
        let connection_properties = ConnectionProperties::default();
        let connection = Connection::connect(uri, connection_properties).await?;

        let mut channel = connection.create_channel().await?;
        let channel = bind_exchange(&mut channel, exchange).await?;

        let publisher = Self {
            channel: Arc::new(channel),
            exchange_name: exchange.to_owned(),
        };

        Ok(publisher)
    }

    /// Publishes a message to the specified destination queue.
    /// **Parameters**
    /// * `message`: The message to be published.
    /// * `payload`: The message payload.
    /// * `routing_key`: The routing key for the message.
    /// **Returns**
    /// * `anyhow::Result<()>` - Returns an empty result if the message was successfully published.
    #[instrument(skip(self, message))]
    pub async fn publish<T, K>(&self, message: &T, routing_key: K) -> anyhow::Result<()>
    where
        T: Serialize,
        K: Into<String> + Debug,
    {
        let payload = serde_json::to_string(&message)?;
        let publish_options = BasicPublishOptions {
            mandatory: true,
            immediate: false,
        };

        let properties = BasicProperties::default();

        self.channel
            .basic_publish(
                &self.exchange_name,
                &routing_key.into(),
                publish_options,
                payload.as_bytes(),
                properties,
            )
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

    channel
        .exchange_declare(
            exchange_name,
            ExchangeKind::Direct,
            exchange_declare_options,
            FieldTable::default(),
        )
        .await?;

    Ok(channel.to_owned())
}
