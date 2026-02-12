use anyhow::Result;
use std::time::Duration;
use tracing::warn;

use super::adapter::{ReviewAction, ReviewerOutput};
use super::orchestrator::RallyEvent;
use crate::github;
use tokio::sync::mpsc;

/// Options controlling how a review is posted to GitHub.
pub struct PostOptions {
    /// Whether to include the "[AI Rally - Reviewer]" header prefix on comments
    pub include_header: bool,
    /// Whether to post the summary review via submit_review()
    pub post_summary: bool,
}

impl Default for PostOptions {
    fn default() -> Self {
        Self {
            include_header: true,
            post_summary: true,
        }
    }
}

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
    options: &PostOptions,
) -> Result<()> {
    if options.post_summary {
        let app_action = match review.action {
            ReviewAction::Approve => crate::app::ReviewAction::Approve,
            ReviewAction::RequestChanges => crate::app::ReviewAction::RequestChanges,
            ReviewAction::Comment => crate::app::ReviewAction::Comment,
        };

        let app_action_for_fallback = app_action;

        let summary_body = if options.include_header {
            format!("[AI Rally - Reviewer]\n\n{}", review.summary)
        } else {
            review.summary.clone()
        };

        let result = github::submit_review(repo, pr_number, app_action, &summary_body).await;

        if result.is_err()
            && matches!(app_action_for_fallback, crate::app::ReviewAction::Approve)
        {
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
                &summary_body,
            )
            .await?;
        } else {
            result?;
        }
    }

    for comment in &review.comments {
        let body = if options.include_header {
            format!("[AI Rally - Reviewer]\n\n{}", comment.body)
        } else {
            comment.body.clone()
        };
        if let Err(e) = github::create_review_comment(
            repo,
            pr_number,
            head_sha,
            &comment.path,
            comment.line,
            &body,
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
