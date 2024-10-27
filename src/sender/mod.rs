mod watcher;

use anyhow::{anyhow, bail, Context};
use bytes::Bytes;
use futures::stream::{SplitSink, StreamExt};
use futures::SinkExt;
use std::path::Path;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

use crate::core::compression::compress_dir;
use crate::core::file_change::{FileChange, SortedFileChanges};
use crate::core::file_tree::FileTree;
use crate::core::message::{FileChangeMessage, RequestMessage};

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

        let files_req = read
            .next()
            .await
            .ok_or(anyhow!("unexpected end of stream"))?
            .map(|req| {
                if let Message::Binary(files_req) = req {
                    bincode::deserialize::<Vec<RequestMessage>>(&files_req)
                        .context("deserializing the initial files request")
                } else {
                    bail!("incorrect file request received, expected binary message")
                }
            })??;

        self.handle_files_req(&mut write, files_req).await;
        println!("Initial sync completed");

        if watch {
            println!("Watching for changes");
            self.watch_dir(&mut write).await?;
        } else {
            write.close().await?;
        }

        Ok(())
    }

    async fn handle_files_req(
        &self,
        write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        requests: Vec<RequestMessage>,
    ) {
        let mut handles = Vec::with_capacity(requests.len());
        for request in requests {
            match request {
                RequestMessage::File(path) => {
                    let file_path = self.dir_path.as_ref().join(&path);
                    handles.push(tokio::spawn(async {
                        let contents = tokio::fs::read(file_path).await.unwrap();
                        let message = FileChangeMessage::FileEdited(path, Bytes::from(contents));

                        bincode::serialize(&message).unwrap()
                    }))
                }
                RequestMessage::Dir(path) => {
                    let dir_path = self.dir_path.as_ref().join(&path);
                    handles.push(tokio::spawn(async {
                        let contents = compress_dir(dir_path).await;

                        let contents = contents.unwrap();
                        let message = FileChangeMessage::DirectoryCreated(path, contents);

                        bincode::serialize(&message).unwrap()
                    }))
                }
            }
        }

        for handle in handles {
            let encoded = handle.await;
            if let Ok(encoded) = encoded {
                if let Err(err) = write.send(Message::Binary(encoded)).await {
                    eprintln!("error occurred while sending message: {}", err);
                }
            }
        }
    }

    async fn watch_dir(
        &self,
        write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
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
                    self.handle_file_changes(write, files).await;
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
