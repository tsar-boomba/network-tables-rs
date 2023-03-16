use std::net::{Ipv4Addr, SocketAddrV4};

use network_tables::v4::subscription::SubscriptionOptions;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("debug,network_tables=debug")
        .init();
    let client = network_tables::v4::Client::try_new_w_config(
        //SocketAddrV4::new(Ipv4Addr::new(10, 35, 6, 2), 5810),
        SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5810),
        network_tables::v4::client_config::Config {
            ..Default::default()
        },
    )
    .await?;
    let published_topic = client
        .publish_topic("/MyTopic", network_tables::v4::Type::Int, None)
        .await?;

    let mut subscription = client
        .subscribe_w_options(
            &[""],
            Some(SubscriptionOptions {
                all: Some(true),
                prefix: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    let task_client = client.clone();
    tokio::spawn(async move {
        let mut counter = 0;
        loop {
            task_client
                .publish_value(&published_topic, &network_tables::Value::from(counter))
                .await
                .unwrap();
            counter += 1;
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
    });

    while let Some(message) = subscription.next().await {
        tracing::info!("{:?}", message);
    }

    Ok(())
}
