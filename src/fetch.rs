//! Reading responses from the servers Stor calls (Payroof servers, Google).
use serde_json::Value;

/// Most bytes of a response body that are read.
const MAX_BYTES: usize = 2 * 1024 * 1024;

/// The JSON body of `response`, read in chunks so an oversized body is refused early.
pub async fn json(mut response: reqwest::Response) -> anyhow::Result<Value> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(body.len() + chunk.len() <= MAX_BYTES, "response too large");
        body.extend(chunk);
    }
    Ok(serde_json::from_slice(&body)?)
}
