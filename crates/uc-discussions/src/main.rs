#![forbid(unsafe_code)]

use uc_discussions::{manifest, DiscussionsHandler};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("ck-uc-discussions {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--help" | "-h" => {
                println!("Usage: ck-uc-discussions --subc <path>\n\nOptions:\n  --subc <path>  Daemon connection file\n  -V, --version  Print version\n  -h, --help     Print help");
                return Ok(());
            }
            _ => {}
        }
    }
    let handler = DiscussionsHandler::try_default()?;
    subc_client_rs::serve(manifest(), handler).await?;
    Ok(())
}
