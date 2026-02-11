use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::adapter::ReviewerOutput;
use super::session::rally_dir;

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingReview {
    pub version: u32,
    pub repo: String,
    pub pr_number: u32,
    pub head_sha: String,
    pub base_branch: String,
    pub created_at: String,
    pub review: ReviewerOutput,
}

pub fn write_pending_review(review: &PendingReview) -> Result<PathBuf> {
    let dir = rally_dir(&review.repo, review.pr_number)?;
    fs::create_dir_all(&dir).context("Failed to create rally directory")?;

    let path = dir.join("pending_review.json");
    let content =
        serde_json::to_string_pretty(review).context("Failed to serialize pending review")?;

    // Atomic write via temp file
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content).context("Failed to write temporary pending review file")?;

    if let Err(e) = fs::rename(&temp_path, &path) {
        let _ = fs::remove_file(&temp_path);
        return Err(e).context("Failed to rename pending review file");
    }

    Ok(path)
}

pub fn read_pending_review(path: &Path) -> Result<PendingReview> {
    let content = fs::read_to_string(path).context("Failed to read pending review file")?;
    let review: PendingReview =
        serde_json::from_str(&content).context("Failed to parse pending review file")?;

    if review.version != CURRENT_VERSION {
        return Err(anyhow!(
            "Unsupported pending review version: {} (expected {})",
            review.version,
            CURRENT_VERSION
        ));
    }

    Ok(review)
}
