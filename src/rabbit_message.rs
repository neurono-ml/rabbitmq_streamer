use std::sync::Arc;

use lapin::{
    message::Delivery,
    options::{BasicAckOptions, BasicNackOptions, BasicRejectOptions},
};

/// A message container that holds the message payload and can its delivery information,
/// that can be acknowledged or rejected.
pub struct AckableMessage<T> {
    delivery: Delivery,
    payload: Arc<T>,
}

impl<T> AckableMessage<T> {
    pub fn new(payload: T, delivery: Delivery) -> Self {
        AckableMessage {
            delivery,
            payload: Arc::new(payload),
        }
    }

    pub async fn ack(&self) -> anyhow::Result<()> {
        let options = BasicAckOptions { multiple: false };
        self.delivery.acker.ack(options).await?;
        Ok(())
    }

    pub async fn nack(&self) -> anyhow::Result<()> {
        let options = BasicNackOptions {
            requeue: true,
            multiple: false,
        };
        self.delivery.acker.nack(options).await?;
        Ok(())
    }

    pub async fn reject(&self) -> anyhow::Result<()> {
        let options = BasicRejectOptions { requeue: false };
        self.delivery.acker.reject(options).await?;
        Ok(())
    }

    pub fn message(&self) -> T
    where
        T: Clone,
    {
        let message = (*self.payload).clone();
        message
    }
}
