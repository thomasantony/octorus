use crate::ai::adapter::{Context, ReviewerOutput};

/// Build the initial reviewer prompt
///
/// If `custom_prompt` is provided, it will be prepended to the default prompt.
pub fn build_reviewer_prompt(
    context: &Context,
    iteration: u32,
    custom_prompt: Option<&str>,
) -> String {
    let pr_body = context
        .pr_body
        .as_deref()
        .unwrap_or("(No description provided)");

    let custom_section = custom_prompt
        .map(|p| format!("## Custom Instructions\n\n{}\n\n", p))
        .unwrap_or_default();

    format!(
        r#"{custom_section}You are a code reviewer for a GitHub Pull Request.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

### PR Description
{pr_body}

### Diff
```diff
{diff}
```

## Your Task

This is iteration {iteration} of the review process.

1. Carefully review the changes in the diff
2. Check for:
   - Code quality issues
   - Potential bugs
   - Security vulnerabilities
   - Performance concerns
   - Style and consistency issues
   - Missing tests or documentation

3. Provide your review decision:
   - "approve" if the changes are good to merge
   - "request_changes" if there are issues that must be fixed
   - "comment" if you have suggestions but they're not blocking

4. List any blocking issues that must be resolved before approval

## Output Format

You MUST respond with a JSON object matching the schema provided.
Be specific in your comments with file paths and line numbers."#,
        custom_section = custom_section,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        pr_body = pr_body,
        diff = context.diff,
        iteration = iteration,
    )
}

/// Build the reviewee prompt based on review feedback
///
/// If `custom_prompt` is provided, it will be prepended to the default prompt.
pub fn build_reviewee_prompt(
    context: &Context,
    review: &ReviewerOutput,
    iteration: u32,
    custom_prompt: Option<&str>,
) -> String {
    let comments_text = review
        .comments
        .iter()
        .map(|c| {
            format!(
                "- [{severity:?}] {path}:{line}: {body}",
                severity = c.severity,
                path = c.path,
                line = c.line,
                body = c.body
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let blocking_text = if review.blocking_issues.is_empty() {
        "None".to_string()
    } else {
        review
            .blocking_issues
            .iter()
            .map(|i| format!("- {}", i))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let custom_section = custom_prompt
        .map(|p| format!("## Custom Instructions\n\n{}\n\n", p))
        .unwrap_or_default();

    format!(
        r#"{custom_section}You are a developer fixing code based on review feedback.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

## Review Feedback (Iteration {iteration})

### Summary
{summary}

### Review Action: {action:?}

### Comments
{comments}

### Blocking Issues
{blocking}

## Your Task

1. Address each blocking issue and review comment
2. Make the necessary code changes
3. If something is unclear, set status to "needs_clarification" and ask a question
4. If you need permission for a significant change, set status to "needs_permission"

## Output Format

You MUST respond with a JSON object matching the schema provided.
List all files you modified in the "files_modified" array."#,
        custom_section = custom_section,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        iteration = iteration,
        summary = review.summary,
        action = review.action,
        comments = comments_text,
        blocking = blocking_text,
    )
}

/// Build a prompt for asking the reviewer a clarification question
#[allow(dead_code)]
pub fn build_clarification_prompt(question: &str) -> String {
    format!(
        r#"The developer has a question about your review feedback:

## Question
{question}

Please provide a clear answer to help them proceed with the fixes.
After answering, provide an updated review if needed."#,
        question = question,
    )
}

/// Build a prompt for continuing after permission is granted
#[allow(dead_code)]
pub fn build_permission_granted_prompt(action: &str) -> String {
    format!(
        r#"Permission has been granted for the following action:

{action}

Please proceed with the implementation."#,
        action = action,
    )
}

/// Build a re-review prompt after fixes
pub fn build_rereview_prompt(context: &Context, iteration: u32, changes_summary: &str) -> String {
    format!(
        r#"The developer has made changes based on your review feedback.

## Context

Repository: {repo}
PR #{pr_number}: {pr_title}

## Changes Made (Iteration {iteration})
{changes_summary}

## Your Task

1. Re-review the changes
2. Check if the blocking issues have been addressed
3. Look for any new issues introduced by the fixes
4. Decide if the PR is now ready to merge

## Output Format

You MUST respond with a JSON object matching the schema provided."#,
        repo = context.repo,
        pr_number = context.pr_number,
        pr_title = context.pr_title,
        iteration = iteration,
        changes_summary = changes_summary,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::{CommentSeverity, ReviewAction, ReviewComment};

    #[test]
    fn test_build_reviewer_prompt() {
        let context = Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: Some("This adds a new feature".to_string()),
            diff: "+added line\n-removed line".to_string(),
            working_dir: None,
        };

        let prompt = build_reviewer_prompt(&context, 1, None);
        assert!(prompt.contains("owner/repo"));
        assert!(prompt.contains("PR #123"));
        assert!(prompt.contains("Add feature"));
        assert!(prompt.contains("iteration 1"));

        // Test with custom prompt
        let prompt_with_custom =
            build_reviewer_prompt(&context, 1, Some("Focus on security issues"));
        assert!(prompt_with_custom.contains("Focus on security issues"));
        assert!(prompt_with_custom.contains("Custom Instructions"));
    }

    #[test]
    fn test_build_reviewee_prompt() {
        let context = Context {
            repo: "owner/repo".to_string(),
            pr_number: 123,
            pr_title: "Add feature".to_string(),
            pr_body: None,
            diff: "".to_string(),
            working_dir: None,
        };

        let review = ReviewerOutput {
            action: ReviewAction::RequestChanges,
            summary: "Please fix the issues".to_string(),
            comments: vec![ReviewComment {
                path: "src/main.rs".to_string(),
                line: 10,
                body: "Missing error handling".to_string(),
                severity: CommentSeverity::Major,
            }],
            blocking_issues: vec!["Fix error handling".to_string()],
        };

        let prompt = build_reviewee_prompt(&context, &review, 1, None);
        assert!(prompt.contains("src/main.rs:10"));
        assert!(prompt.contains("Missing error handling"));
        assert!(prompt.contains("Fix error handling"));

        // Test with custom prompt
        let prompt_with_custom = build_reviewee_prompt(
            &context,
            &review,
            1,
            Some("Run cargo fmt before committing"),
        );
        assert!(prompt_with_custom.contains("Run cargo fmt before committing"));
        assert!(prompt_with_custom.contains("Custom Instructions"));
    }
}
