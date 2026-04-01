use std::time::Duration;

use crossterm::event::{Event as CEvent, EventStream};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::message::Message;

pub fn spawn(tx: mpsc::Sender<Message>) {
    let tx_input = tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            if let CEvent::Key(key) = event
                && tx_input.send(Message::Key(key)).await.is_err()
            {
                break;
            }
        }
    });

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            if tx.send(Message::Tick).await.is_err() {
                break;
            }
        }
    });
}
