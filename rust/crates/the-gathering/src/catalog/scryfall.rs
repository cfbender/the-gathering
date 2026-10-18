//! The Scryfall bulk-data downloads the catalog sync makes. Requests go through lotus's
//! [`ScryfallClient`]; everything else reads the imported data from SQLite.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lotus::scryfall::{ScryfallClient, ScryfallError};

use crate::db::UtcDateTime;

/// The Scryfall client.
#[derive(Debug)]
pub struct Scryfall {
    client: ScryfallClient,
}

impl Scryfall {
    /// A client for `api_base`.
    pub fn new(api_base: &str) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::state::user_agent())
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client: ScryfallClient::with_client(http, api_base.trim_end_matches('/')),
        })
    }

    /// Downloads the bulk file of type `kind` (such as `default_cards` or `rulings`) into
    /// `dir`, returning its path and Scryfall's generation time.
    pub async fn download_bulk(
        &self,
        dir: &Path,
        kind: &str,
    ) -> anyhow::Result<(PathBuf, Option<UtcDateTime>)> {
        let list = self.client.bulk_data_list().await?;
        let metadata = list
            .find(kind)
            .ok_or_else(|| anyhow::anyhow!("Scryfall did not return {kind} bulk metadata"))?;
        let uri = metadata
            .download_uri()
            .map_err(|_| anyhow::anyhow!("Scryfall {kind} has no JSONL URI"))?;
        let path = dir.join(format!(
            "the-gathering-scryfall-{kind}-{}.jsonl.gz",
            uuid::Uuid::new_v4()
        ));
        match self.client.download_to_file(uri, &path).await {
            Ok(_) => Ok((path, metadata.updated_at.map(UtcDateTime::from_offset))),
            Err(ScryfallError::Status(status)) => {
                anyhow::bail!(
                    "Scryfall {kind} bulk download returned HTTP {}",
                    status.as_u16()
                )
            }
            Err(error) => Err(error.into()),
        }
    }
}
