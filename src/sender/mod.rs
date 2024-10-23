mod watcher;

use anyhow::{anyhow, bail};
use bytes::Bytes;
use futures::stream::{SplitSink, StreamExt};
use futures::SinkExt;
use std::path::{Path, PathBuf};
use std::process;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

use crate::core::file_change::{FileChange, SortedFileChanges};
use crate::core::file_tree::FileTree;
use crate::core::message::FileChangeMessage;

pub struct Sender<'command, P: AsRef<Path>> {
    listener_addr: &'command str,
    dir_path: P,
}

impl<'command, P: AsRef<Path>> Sender<'command, P> {
    pub fn new(dir_path: P, listener_addr: &'command str) -> Self {
        Self {
            listener_addr,
            dir_path,
        }
    }

    pub async fn start(&self, watch: bool) -> anyhow::Result<()> {
        let tree = FileTree::new(&self.dir_path).await?;
        let request = self.listener_addr.into_client_request()?;
        let (stream, _response) = connect_async(request).await?;
        let (mut write, mut read) = stream.split();

        let encoded = bincode::serialize(&tree)?;
        println!("Sending initial directory state");
        write.send(Message::Binary(encoded)).await?;
        println!("Initial state sent, starting sync");

        let res = read
            .next()
            .await
            .ok_or(anyhow!("unexpected end of stream"))??;

        if let Message::Binary(bytes) = res {
            let requested_files: Vec<PathBuf> = bincode::deserialize(bytes.as_slice())?;
            let mut handles = Vec::with_capacity(requested_files.len());
            for path in requested_files {
                let file_path = self.dir_path.as_ref().join(path);
                handles.push(tokio::spawn(async {
                    let contents = tokio::fs::read(&file_path).await.unwrap();
                    (file_path, Bytes::from(contents))
                }))
            }

            for handle in handles {
                let res = handle.await;
                if let Ok((path, contents)) = res {
                    let truncated_path = path.strip_prefix(&self.dir_path).unwrap().to_owned();
                    let message = FileChangeMessage::FileEdited(truncated_path, contents);
                    let encoded = bincode::serialize(&message).unwrap();
                    if let Err(err) = write.send(Message::Binary(encoded)).await {
                        eprintln!("error occurred while sending message: {}", err);
                    }
                }
            }
        } else {
            bail!("Incorrect message format received from listener, expected binary message");
        }

        println!("Initial sync completed");

        if watch {
            println!("Watching for changes");
            self.watch_dir(write).await?;
        } else {
            write.close().await?;
        }

        Ok(())
    }

    async fn watch_dir(
        &self,
        mut write: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    ) -> anyhow::Result<()> {
        let mut subscription = watcher::watch_dir(self.dir_path.as_ref()).await?;

        loop {
            tokio::select! {
                Ok(data) = subscription.next() => {
                    let files = match data {
                        watchman_client::SubscriptionData::FilesChanged(res) => res.files,
                        _ => continue,
                    };

                    if files.is_none() || files.as_ref().unwrap().is_empty() {
                        continue;
                    }

                    let files = files.unwrap();
                    self.handle_file_changes(&mut write, files).await;
                }

                _ = tokio::signal::ctrl_c() => {
                    println!("Exiting");
                    write.close().await?;
                    break Ok(());
                }
            }
        }
    }

    async fn handle_file_changes(
        &self,
        write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        files: Vec<FileChange>,
    ) {
        let mut changes = SortedFileChanges::from(self.dir_path.as_ref().to_owned(), files);
        while let Some(message) = changes.next_message().await {
            let encoded = bincode::serialize(&message).unwrap();
            if let Err(err) = write.send(Message::Binary(encoded)).await {
                eprintln!("error occurred while sending message: {}", err);
            }
        }
    }
}
