use anyhow::{anyhow, bail};
use futures::{SinkExt, StreamExt};
use std::path::Path;
use tokio::net::TcpListener;

use crate::core::{
    file_tree::{FileTree, TreeDiff},
    message::FileChangeMessage,
};

pub struct Receiver<P>
where
    P: AsRef<Path>,
{
    port: u32,
    out_dir: P,
}

impl<P> Receiver<P>
where
    P: AsRef<Path>,
{
    pub fn new(port: u32, out_dir: P) -> Self {
        Self { port, out_dir }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let tree = FileTree::new(&self.out_dir).await?;
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("WebSocket server listening on {}", addr.as_str());

        let (stream, _) = listener.accept().await?;
        let socket = tokio_tungstenite::accept_async(stream).await?;
        let (mut write, mut read) = socket.split();

        let initial_message = read
            .next()
            .await
            .ok_or(anyhow!("unexpected end of stream"))??;

        let initial_message = match initial_message {
            tungstenite::Message::Binary(bin) => bin,
            _ => bail!("incorrect initial message format"),
        };

        let remote_tree: FileTree = bincode::deserialize(&initial_message)?;
        let diff = TreeDiff::from(&tree, &remote_tree);
        let requested_files = diff.apply(self.out_dir.as_ref()).await?;
        let encoded = bincode::serialize(&requested_files)?;
        write.send(tungstenite::Message::binary(encoded)).await?;

        while let Some(message) = read.next().await {
            if message.is_err() {
                continue;
            }

            let message: FileChangeMessage = match message.unwrap() {
                tungstenite::Message::Binary(bin) => bincode::deserialize(bin.as_slice()).unwrap(),
                _ => continue,
            };

            let _ = self.handle_message(message).await;
        }

        Ok(())
    }

    async fn handle_message(&self, message: FileChangeMessage) -> anyhow::Result<()> {
        match message {
            FileChangeMessage::FileCreated(path) => {
                let file_path = self.out_dir.as_ref().join(path);
                tokio::fs::File::create(file_path).await?;
            }
            FileChangeMessage::FileDeleted(path) => {
                let file_path = self.out_dir.as_ref().join(path);
                tokio::fs::remove_file(file_path).await?;
            }
            FileChangeMessage::Rename(old_path, new_path) => {
                let from = self.out_dir.as_ref().join(old_path);
                let to = self.out_dir.as_ref().join(new_path);
                tokio::fs::rename(from, to).await?;
            }
            FileChangeMessage::DirectoryCreated(path) => {
                let dir_path = self.out_dir.as_ref().join(path);
                tokio::fs::create_dir(dir_path).await?;
            }
            FileChangeMessage::DirectoryDeleted(path) => {
                let dir_path = self.out_dir.as_ref().join(path);
                tokio::fs::remove_dir_all(dir_path).await?;
            }
            FileChangeMessage::FileEdited(path, contents) => {
                let file_path = self.out_dir.as_ref().join(path);
                tokio::fs::write(file_path, contents).await?;
            }
            FileChangeMessage::DirectoryContentsEdited(_) => (),
        }

        Ok(())
    }
}
