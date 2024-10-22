#![allow(deprecated)]
use std::path::Path;

use crate::core::file_change::FileChange;
use anyhow::anyhow;
use watchman_client::{CanonicalPath, Connector, Subscription};

use watchman_client::prelude::*;

pub async fn watch_dir(path: &Path) -> anyhow::Result<Subscription<FileChange>> {
    let client = Connector::new().connect().await.map_err(|_| {
        anyhow!("could not connect to watchman server, make sure it is installed on your system")
    })?;

    let path = CanonicalPath::canonicalize(path)?;

    let resolved = client.resolve_root(path).await?;

    let (subscription, _) = client
        .subscribe::<FileChange>(
            &resolved,
            SubscribeRequest {
                empty_on_fresh_instance: true,
                expression: Some(Expr::Any(vec![
                    Expr::FileType(FileType::Regular),
                    Expr::FileType(FileType::Directory),
                ])),
                ..Default::default()
            },
        )
        .await?;

    Ok(subscription)
}
