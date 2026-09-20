#![forbid(unsafe_code)]

use uc_discussions::{manifest, DiscussionsHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handler = DiscussionsHandler::try_default()?;
    subc_client_rs::serve(manifest(), handler).await?;
    Ok(())
}
