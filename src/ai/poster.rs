use anyhow::Result;
use std::time::Duration;
use tracing::warn;

use super::adapter::{ReviewAction, ReviewerOutput};
use super::orchestrator::RallyEvent;
use crate::github;
use tokio::sync::mpsc;

/// Post a reviewer's output to GitHub as a PR review with inline comments.
///
/// This is the shared posting logic used by both the Orchestrator (live mode)
/// and the `or post` CLI subcommand.
pub async fn post_review_to_github(
    repo: &str,
    pr_number: u32,
    head_sha: &str,
    review: &ReviewerOutput,
    event_sink: Option<&mpsc::Sender<RallyEvent>>,
) -> Result<()> {
    let app_action = match review.action {
        ReviewAction::Approve => crate::app::ReviewAction::Approve,
        ReviewAction::RequestChanges => crate::app::ReviewAction::RequestChanges,
        ReviewAction::Comment => crate::app::ReviewAction::Comment,
    };

    let app_action_for_fallback = app_action;

    let summary_with_prefix = format!("[AI Rally - Reviewer]\n\n{}", review.summary);

    let result =
        github::submit_review(repo, pr_number, app_action, &summary_with_prefix).await;

    if result.is_err() && matches!(app_action_for_fallback, crate::app::ReviewAction::Approve) {
        warn!("Approve failed, falling back to comment");
        if let Some(sink) = event_sink {
            let _ = sink
                .send(RallyEvent::Log(
                    "Approve failed, falling back to comment".to_string(),
                ))
                .await;
        }
        github::submit_review(
            repo,
            pr_number,
            crate::app::ReviewAction::Comment,
            &summary_with_prefix,
        )
        .await?;
    } else {
        result?;
    }

    for comment in &review.comments {
        let body_with_prefix = format!("[AI Rally - Reviewer]\n\n{}", comment.body);
        if let Err(e) = github::create_review_comment(
            repo,
            pr_number,
            head_sha,
            &comment.path,
            comment.line,
            &body_with_prefix,
        )
        .await
        {
            warn!(
                "Failed to post inline comment on {}:{}: {}",
                comment.path, comment.line, e
            );
            if let Some(sink) = event_sink {
                let _ = sink
                    .send(RallyEvent::Log(format!(
                        "Warning: Failed to post inline comment on {}:{}: {}",
                        comment.path, comment.line, e
                    )))
                    .await;
            }
        }
        // Rate limit mitigation
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}
