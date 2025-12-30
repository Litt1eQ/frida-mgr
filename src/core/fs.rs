use crate::core::error::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub async fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 8192];

    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

pub async fn ensure_dir_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        tokio::fs::create_dir_all(path).await?;
    }
    Ok(())
}

pub async fn decompress_xz(input: &Path, output: &Path) -> Result<()> {
    use std::io::BufReader;
    use tokio::task;
    use xz2::read::XzDecoder;

    let input = input.to_path_buf();
    let output = output.to_path_buf();

    task::spawn_blocking(move || {
        let file = std::fs::File::open(&input)?;
        let buf_reader = BufReader::new(file);
        let mut decoder = XzDecoder::new(buf_reader);
        let mut output_file = std::fs::File::create(&output)?;
        std::io::copy(&mut decoder, &mut output_file)?;
        Ok::<_, std::io::Error>(())
    })
    .await
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))??;

    Ok(())
}

#[cfg(unix)]
pub async fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = tokio::fs::metadata(path).await?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    tokio::fs::set_permissions(path, permissions).await?;
    Ok(())
}

#[cfg(not(unix))]
pub async fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}
