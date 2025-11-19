use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use lapin::{
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
};
use serde::Serialize;
use std::{fmt::Debug, sync::Arc, time::Duration};
use tracing::{instrument, warn};

/// A struct representing a RabbitMQ publisher. It holds the necessary information to publish messages to an exchange.
/// **Parameters**
/// * `channel`: A reference-counted handle to a channel, which is used to send messages.
/// * `exchange_name`: The name of the exchange where messages will be published.
pub struct RabbitPublisher {
    channel: Arc<Channel>,
    exchange_name: String,
    backoff: ExponentialBackoff,
}

/// Builder for RabbitPublisher with configurable backoff parameters
pub struct RabbitPublisherBuilder {
    uri: String,
    exchange: String,
    max_elapsed_time: Option<Duration>,
    initial_interval: Option<Duration>,
    max_interval: Option<Duration>,
    multiplier: Option<f64>,
    randomization_factor: Option<f64>,
}

impl RabbitPublisherBuilder {
    /// Creates a new builder with required parameters
    pub fn new(uri: impl Into<String>, exchange: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            exchange: exchange.into(),
            max_elapsed_time: None,
            initial_interval: None,
            max_interval: None,
            multiplier: None,
            randomization_factor: None,
        }
    }

    /// Sets the maximum elapsed time for retries (default: 15 minutes)
    /// After this time, the backoff will stop retrying
    pub fn max_elapsed_time(mut self, duration: Duration) -> Self {
        self.max_elapsed_time = Some(duration);
        self
    }

    /// Sets the initial retry interval (default: 500ms)
    pub fn initial_interval(mut self, duration: Duration) -> Self {
        self.initial_interval = Some(duration);
        self
    }

    /// Sets the maximum retry interval (default: 60 seconds)
    pub fn max_interval(mut self, duration: Duration) -> Self {
        self.max_interval = Some(duration);
        self
    }

    /// Sets the multiplier for exponential backoff (default: 1.5)
    pub fn multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = Some(multiplier);
        self
    }

    /// Sets the randomization factor (default: 0.5)
    /// The actual interval will be randomized by ± this factor
    pub fn randomization_factor(mut self, factor: f64) -> Self {
        self.randomization_factor = Some(factor);
        self
    }

    /// Builds the RabbitPublisher with the configured parameters
    pub async fn build(self) -> anyhow::Result<RabbitPublisher> {
        let connection_properties = ConnectionProperties::default();
        let connection = Connection::connect(&self.uri, connection_properties).await?;

        let mut channel = connection.create_channel().await?;
        let channel = bind_exchange(&mut channel, &self.exchange).await?;

        // Build backoff with custom parameters
        let mut backoff_builder = ExponentialBackoffBuilder::default();

        if let Some(duration) = self.max_elapsed_time {
            backoff_builder.with_max_elapsed_time(Some(duration));
        }

        if let Some(duration) = self.initial_interval {
            backoff_builder.with_initial_interval(duration);
        }

        if let Some(duration) = self.max_interval {
            backoff_builder.with_max_interval(duration);
        }

        if let Some(multiplier) = self.multiplier {
            backoff_builder.with_multiplier(multiplier);
        }

        if let Some(factor) = self.randomization_factor {
            backoff_builder.with_randomization_factor(factor);
        }

        let backoff = backoff_builder.build();

        let publisher = RabbitPublisher {
            channel: Arc::new(channel),
            exchange_name: self.exchange,
            backoff,
        };

        Ok(publisher)
    }
}

impl RabbitPublisher {
    /// Connects to a RabbitMQ broker and binds the specified exchange. Use app_group_namespace
    /// to identify your application while creating a routing key.
    /// 
    /// This method uses default backoff configuration. For custom backoff settings,
    /// use `RabbitPublisherBuilder::new(uri, exchange).build().await`
    /// 
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
        RabbitPublisherBuilder::new(uri, exchange).build().await
    }
    
    /// Creates a builder for configuring the publisher with custom backoff parameters
    /// 
    /// # Example
    /// ```no_run
    /// use rabbitmq_streamer::RabbitPublisher;
    /// use std::time::Duration;
    /// 
    /// # async fn example() -> anyhow::Result<()> {
    /// let publisher = RabbitPublisher::builder("amqp://localhost", "my_exchange")
    ///     .max_elapsed_time(Duration::from_secs(30))
    ///     .initial_interval(Duration::from_millis(100))
    ///     .max_interval(Duration::from_secs(10))
    ///     .multiplier(2.0)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn builder(uri: impl Into<String>, exchange: impl Into<String>) -> RabbitPublisherBuilder {
        RabbitPublisherBuilder::new(uri, exchange)
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
        let routing_key_str = routing_key.into();
        let publish_options = BasicPublishOptions {
            mandatory: true,
            immediate: false,
        };

        let properties = BasicProperties::default();

        let channel = self.channel.clone();
        let exchange_name = self.exchange_name.clone();
        
        backoff::future::retry_notify(
            self.backoff.clone(),
            || async {
                // Check if channel is connected before attempting to publish
                if !channel.status().connected() {
                    warn!("Channel is not connected, returning transient error for retry");
                    return Err(backoff::Error::transient(
                        anyhow::anyhow!("Channel not connected")
                    ));
                }
                
                channel
                    .basic_publish(
                        &exchange_name,
                        &routing_key_str,
                        publish_options,
                        payload.as_bytes(),
                        properties.clone(),
                    )
                    .await
                    .map_err(|e| backoff::Error::transient(anyhow::Error::from(e)))
            },
            |err, duration| {
                warn!(
                    "Failed to publish message, retrying in {:?}: {:?}",
                    duration, err
                );
            },
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
