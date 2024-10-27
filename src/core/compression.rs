use std::path::Path;

use anyhow::Context;
use bytes::Bytes;

pub async fn compress_dir(path: impl AsRef<Path>) -> anyhow::Result<Bytes> {
    let mut tar = async_tar::Builder::new(Vec::new());

    tar.append_dir_all(".", path.as_ref())
        .await
        .context("compressing dir")?;
    let inner = tar.into_inner().await.context("finalzing archive")?;
    let inner_bytes = Bytes::from(inner);

    dbg!(path.as_ref(), &inner_bytes);

    Ok(inner_bytes)
}

pub async fn decompress_dir(path: impl AsRef<Path>, compressed: &[u8]) -> anyhow::Result<()> {
    let ar = async_tar::Archive::new(compressed);
    ar.unpack(path.as_ref())
        .await
        .context("decompressing dir")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;
    use tokio::test;

    async fn create_test_files(dir: &Path) -> anyhow::Result<()> {
        // Create a few test files with different content
        fs::create_dir_all(dir.join("subdir"))?;

        let files = vec![
            ("test1.txt", "Hello, World!"),
            ("test2.txt", "Another test file"),
            ("subdir/nested.txt", "Nested file content"),
        ];

        for (path, content) in files {
            let mut file = File::create(dir.join(path))?;
            file.write_all(content.as_bytes())?;
        }

        Ok(())
    }

    async fn verify_files(dir: &Path) -> anyhow::Result<()> {
        let expected_contents = vec![
            ("test1.txt", "Hello, World!"),
            ("test2.txt", "Another test file"),
            ("subdir/nested.txt", "Nested file content"),
        ];

        for (path, expected_content) in expected_contents {
            let content = fs::read_to_string(dir.join(path))?;
            assert_eq!(content, expected_content);
        }

        Ok(())
    }

    #[test]
    async fn test_compress_decompress_cycle() -> anyhow::Result<()> {
        // Create a temporary directory for source files
        let source_dir = TempDir::new()?;
        create_test_files(source_dir.path()).await?;

        // Compress the directory
        let compressed = compress_dir(source_dir.path()).await?;

        // Create a temporary directory for decompressed files
        let output_dir = TempDir::new()?;

        // Decompress the files
        decompress_dir(output_dir.path(), &compressed).await?;

        // Verify the contents
        verify_files(output_dir.path()).await?;

        Ok(())
    }

    #[test]
    async fn test_empty_directory() -> anyhow::Result<()> {
        let empty_dir = TempDir::new()?;

        // Compress empty directory
        let compressed = compress_dir(empty_dir.path()).await?;

        // Create output directory and decompress
        let output_dir = TempDir::new()?;
        decompress_dir(output_dir.path(), &compressed).await?;

        // Verify the directory exists and is empty
        assert!(output_dir.path().exists());
        assert!(fs::read_dir(output_dir.path())?.next().is_none());

        Ok(())
    }

    #[test]
    async fn test_large_files() -> anyhow::Result<()> {
        let source_dir = TempDir::new()?;

        // Create a large file (1MB)
        let large_data = vec![b'A'; 1024 * 1024];
        let file_path = source_dir.path().join("large.txt");
        fs::write(&file_path, &large_data)?;

        // Compress and decompress
        let compressed = compress_dir(source_dir.path()).await?;
        let output_dir = TempDir::new()?;
        decompress_dir(output_dir.path(), &compressed).await?;

        // Verify the large file
        let decompressed_data = fs::read(output_dir.path().join("large.txt"))?;
        assert_eq!(decompressed_data, large_data);

        Ok(())
    }

    #[test]
    async fn test_invalid_compressed_data() {
        let output_dir = TempDir::new().unwrap();
        let invalid_data = b"not a valid tar archive";

        let result = decompress_dir(output_dir.path(), invalid_data).await;
        assert!(result.is_err());
    }

    #[test]
    async fn test_nonexistent_source_directory() {
        let nonexistent_path = Path::new("/path/that/does/not/exist");
        let result = compress_dir(nonexistent_path).await;
        assert!(result.is_err());
    }
}
