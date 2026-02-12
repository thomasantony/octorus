use anyhow::Result;
use std::path::Path;

use octorus::ai::pending_review::read_pending_review;
use octorus::ai::poster::post_review_to_github;

pub async fn run_post(file_path: &str) -> Result<()> {
    let path = Path::new(file_path);
    let pending = read_pending_review(path)?;

    println!(
        "Posting review to {}/pull/{}",
        pending.repo, pending.pr_number
    );
    println!("  Action: {:?}", pending.review.action);
    println!("  Summary: {}", truncate(&pending.review.summary, 80));
    println!("  Inline comments: {}", pending.review.comments.len());
    println!("  Head SHA: {}", &pending.head_sha[..8.min(pending.head_sha.len())]);
    println!();

    post_review_to_github(
        &pending.repo,
        pending.pr_number,
        &pending.head_sha,
        &pending.review,
        None,
        &octorus::ai::poster::PostOptions::default(),
    )
    .await?;

    println!("Review posted successfully!");
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
