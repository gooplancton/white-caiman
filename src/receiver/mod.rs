use anyhow::{bail, Context};
use futures::{SinkExt, StreamExt};
use std::path::Path;
use tokio::net::{TcpListener, TcpStream};

use crate::core::{
    compression::decompress_dir, file_tree::FileTree, file_tree_diff::TreeDiff,
    message::FileChangeMessage,
};

pub struct Receiver<P: AsRef<Path>> {
    port: u32,
    out_dir: P,
}

impl<P: AsRef<Path>> Receiver<P> {
    pub fn new(port: u32, out_dir: P) -> Self {
        Self { port, out_dir }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let tree = FileTree::new(&self.out_dir).await?;
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        println!("WebSocket server listening on {}", addr.as_str());

        tokio::select! {
            res = listener.accept() => {
                let (stream, _) = res.unwrap();
                self.sync_dir(&tree, stream).await?
            }

            _ = tokio::signal::ctrl_c() => {
                println!("Shutting down gracefully");
            }
        };

        Ok(())
    }

    async fn sync_dir(&self, tree: &FileTree, stream: TcpStream) -> anyhow::Result<()> {
        let socket = tokio_tungstenite::accept_async(stream).await?;
        let (mut write, mut read) = socket.split();

        let initial_message = read
            .next()
            .await
            .context("Unexpected end of stream, sender did not send initial directoy state")??;

        let initial_message = match initial_message {
            tungstenite::Message::Binary(bin) => bin,
            _ => bail!("Incorrect initial message format, expected binary message"),
        };

        let remote_tree: FileTree = bincode::deserialize(&initial_message)?;
        if !remote_tree.is_valid() {
            bail!("Invalid file tree received, aborting")
        }

        let diff = TreeDiff::from(tree, &remote_tree);
        let requested_files = diff.apply(self.out_dir.as_ref()).await;
        println!("Initial sync completed\n{}", &diff);

        let encoded = bincode::serialize(&requested_files)?;
        write.send(tungstenite::Message::binary(encoded)).await?;

        while let Some(message) = read.next().await {
            if message.is_err() {
                continue;
            }

            let message: FileChangeMessage = match message.as_ref().unwrap() {
                tungstenite::Message::Binary(bin) => bincode::deserialize(bin).unwrap(),
                tungstenite::Message::Close(_) => {
                    println!("Stream closed, exiting");
                    break;
                }
                _ => {
                    eprintln!("Received non-binary message, ignoring");
                    continue;
                }
            };

            if let Err(err) = self.handle_message(message).await {
                eprintln!("An error occurred while handling message: {}", err);
            };
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
            FileChangeMessage::EmptyDirectoryCreated(path) => {
                let dir_path = self.out_dir.as_ref().join(path);
                tokio::fs::create_dir(dir_path).await?;
            }
            FileChangeMessage::DirectoryCreated(path, compressed) => {
                let dir_path = self.out_dir.as_ref().join(path);
                tokio::fs::create_dir(dir_path.as_path()).await?;
                decompress_dir(dir_path.as_path(), compressed.as_ref()).await?;
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
