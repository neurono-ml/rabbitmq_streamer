use anyhow::Context;
use crate::RabbitConsumer;

#[derive(Debug, Default)]
pub struct RabbitConsumerBuilder {
    uri: Option<String>,
    queue_name: Option<String>,
    consumer_tag: Option<String>,
}


impl RabbitConsumerBuilder {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_uri<S>(mut self, uri: S) -> Self
    where
        S: Into<String>
    {
        self.uri = Some(uri.into());
        self
    }

    pub fn with_queue_name<S>(mut self, queue_name: S) -> Self
    where
        S: Into<String>
    {
        self.queue_name = Some(queue_name.into());
        self
    }

    pub fn with_consumer_tag<S>(mut self, consumer_tag: S) -> Self
    where
        S: Into<String>
    {
        self.consumer_tag = Some(consumer_tag.into());
        self
    }

    pub fn build(self) -> anyhow::Result<RabbitConsumer> {
        let uri = self.uri.context("uri is required")?;
        let queue_name = self.queue_name.context("queue_name is required")?;
        let consumer_tag = if let Some(consumer_tag) = self.consumer_tag {
            consumer_tag
        } else {
            uuid::Uuid::new_v4().to_string()
        };

        let consumer = RabbitConsumer { uri, queue_name, consumer_tag };
        
        Ok(consumer)
    }
}

