use media_webhook_retry::{delivery_payload, Asset, Infrai};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infrai = Infrai::from_env()?;
    let queue = std::env::var("MEDIA_QUEUE").unwrap_or_else(|_| "media-creator-delivery".into());
    let asset = Asset {
        asset_id: "asset_demo_01".into(),
        creator_id: "creator_42".into(),
        title: "Launch trailer".into(),
    };
    let _ = infrai.queue_create(&queue).await?;
    let result = async {
        let payload = delivery_payload(&asset);
        let published = infrai.queue_publish(&queue, &payload).await?;
        println!("published processed asset: {}", published);
        let messages = infrai.queue_consume(&queue, 1).await?;
        println!("delivery worker received: {}", messages);
        if let Some(message_id) = messages
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("message_id"))
            .and_then(|v| v.as_str())
        {
            infrai.queue_ack(&queue, message_id).await?;
            println!("acknowledged {message_id}");
        }
        if let Ok(webhook_id) = std::env::var("INFRAI_WEBHOOK_ID") {
            println!(
                "delivery history: {}",
                infrai.webhook_deliveries(&webhook_id).await?
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    match (result, infrai.queue_delete(&queue).await) {
        (Err(workflow_error), _) => Err(workflow_error),
        (Ok(()), Err(cleanup_error)) => Err(cleanup_error.into()),
        (Ok(()), Ok(_)) => Ok(()),
    }
}
