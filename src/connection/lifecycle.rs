use std::{future::Future, time::Duration};

use tokio::{io::AsyncWriteExt, net::TcpStream};
use tracing::{error, info};

use crate::connection::{ConnectionEvent, ConnectionIo, relay};

pub async fn run<F, Fut>(connect_fn: F, reconnect_delay: Duration, mut io: ConnectionIo)
where
    F: Fn() -> Fut,
    Fut: Future<Output = anyhow::Result<TcpStream>>,
{
    loop {
        info!("connection: waiting for connection...");
        let stream = match connect_fn().await {
            Ok(s) => s,
            Err(e) => {
                error!("connection: connect failed: {e}");
                tokio::time::sleep(reconnect_delay).await;
                continue;
            }
        };

        let (mut reader, mut writer) = tokio::io::split(stream);

        info!("connection: connected");
        if !io.send_event(ConnectionEvent::Reconnected).await {
            return;
        }

        let should_reconnect = relay::relay(&mut reader, &mut writer, &mut io).await;
        let _ = writer.shutdown().await;

        if !io.send_event(ConnectionEvent::Disconnected).await {
            return;
        }

        if !should_reconnect {
            return;
        }

        info!("connection: reconnecting in {reconnect_delay:?}...");
        tokio::time::sleep(reconnect_delay).await;
    }
}
