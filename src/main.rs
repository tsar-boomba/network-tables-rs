use std::net::{Ipv4Addr, SocketAddrV4};

use network_tables::v4::subscription::SubscriptionOptions;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("debug,network_tables=trace")
        .init();
    let client =
        network_tables::v4::Client::try_new(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5810))
            .await?;

    let mut subscription = client
        .subscribe_w_options(
            &["/"],
            Some(SubscriptionOptions {
                all: Some(true),
                prefix: Some(true),
                ..Default::default()
            }),
        )
        .await?;

    while let Some(message) = subscription.next().await {
        tracing::info!("message from server: {message:#?}");
    }

    Ok(())
}
