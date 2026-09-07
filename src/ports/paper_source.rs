use async_trait::async_trait;

use crate::domain::paper::Paper;
use crate::error::Result;

#[async_trait]
pub trait PaperSource: Send + Sync {
    /// Fetch papers matching a query from an external source.
    async fn fetch_papers(&self, query: &str, limit: usize) -> Result<Vec<Paper>>;

    /// Human-readable name of this source.
    fn name(&self) -> &str;
}
