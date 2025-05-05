use crate::rabbit_message::AckableMessage;

pub trait AckableMessageCollectionExtension<T> {
    fn ack(&self) -> impl std::future::Future<Output = anyhow::Result<()>>;
    fn nack(&self) -> impl std::future::Future<Output = anyhow::Result<()>>;
    fn reject(&self) -> impl std::future::Future<Output = anyhow::Result<()>>;
}

impl<T> AckableMessageCollectionExtension<T>  for Vec<AckableMessage<T>> {
    async fn ack(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.ack().await?;
        }

        Ok(())
    }

    async fn nack(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.nack().await?;
        }

        Ok(())
    }

    async fn reject(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.reject().await?;
        }

        Ok(())
    }
}


impl<T> AckableMessageCollectionExtension<T>  for Option<AckableMessage<T>> {
    async fn ack(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.ack().await?;
        }

        Ok(())
    }

    async fn nack(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.nack().await?;
        }

        Ok(())
    }

    async fn reject(&self) -> anyhow::Result<()> {
        for ackable_message in self.into_iter() {
            ackable_message.reject().await?;
        }

        Ok(())
    }
}