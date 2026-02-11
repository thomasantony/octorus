use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::ai::adapter::CommentSeverity;
use crate::ai::pending_review::PendingReview;
use crate::app::{App, PendingReviewEditState};

pub fn render(frame: &mut Frame, app: &mut App) {
    let (Some(pending), Some(edit_state)) = (&app.pending_review, &app.pending_review_edit) else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Header
            Constraint::Min(10),  // Comment list
            Constraint::Length(3), // Status bar
        ])
        .split(frame.area());

    // Header
    let action_text = format!("{:?}", pending.review.action);
    let total_comments = pending.review.comments.len();
    let deleted_count = edit_state.deleted_comments.len();
    let edited_count = edit_state.edited_bodies.len();

    let header_lines = vec![
        Line::from(vec![
            Span::styled("Repo: ", Style::default().fg(Color::Gray)),
            Span::styled(&pending.repo, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled("PR: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("#{}", pending.pr_number), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Action: ", Style::default().fg(Color::Gray)),
            Span::styled(&action_text, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("Comments: {} ({} deleted, {} edited)", total_comments, deleted_count, edited_count), Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("Summary: ", Style::default().fg(Color::Gray)),
            Span::styled(truncate(&pending.review.summary, 80), Style::default().fg(Color::White)),
        ]),
    ];

    let header = Paragraph::new(header_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Pending Review ")
            .border_style(Style::default().fg(Color::Yellow)),
    );
    frame.render_widget(header, chunks[0]);

    // Comment list
    let visible_height = chunks[1].height.saturating_sub(2) as usize;
    let comments = &pending.review.comments;

    if edit_state.posting {
        let posting_msg = Paragraph::new("Posting review to GitHub...")
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Comments ")
                    .border_style(Style::default().fg(Color::Gray)),
            );
        frame.render_widget(posting_msg, chunks[1]);
    } else if let Some(ref result) = edit_state.post_result {
        let (msg, color) = match result {
            Ok(()) => ("Review posted successfully!".to_string(), Color::Green),
            Err(e) => (format!("Post failed: {}", e), Color::Red),
        };
        let result_msg = Paragraph::new(msg)
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Result ")
                    .border_style(Style::default().fg(color)),
            );
        frame.render_widget(result_msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = comments
            .iter()
            .enumerate()
            .map(|(i, comment)| {
                let is_selected = i == edit_state.selected_comment;
                let is_deleted = edit_state.deleted_comments.contains(&i);
                let is_edited = edit_state.edited_bodies.contains_key(&i);

                let selector = if is_selected { ">" } else { " " };

                let severity_str = match comment.severity {
                    CommentSeverity::Critical => "CRIT",
                    CommentSeverity::Major => "MAJ ",
                    CommentSeverity::Minor => "MIN ",
                    CommentSeverity::Suggestion => "SUGG",
                };
                let severity_color = match comment.severity {
                    CommentSeverity::Critical => Color::Red,
                    CommentSeverity::Major => Color::Yellow,
                    CommentSeverity::Minor => Color::Blue,
                    CommentSeverity::Suggestion => Color::Green,
                };

                let body = if let Some(edited) = edit_state.edited_bodies.get(&i) {
                    edited.as_str()
                } else {
                    &comment.body
                };

                let status_marker = if is_deleted {
                    " [DEL]"
                } else if is_edited {
                    " [EDITED]"
                } else {
                    ""
                };

                let base_style = if is_deleted {
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT)
                } else if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                let mut item = ListItem::new(Line::from(vec![
                    Span::styled(
                        selector.to_string(),
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}] ", severity_str),
                        Style::default().fg(severity_color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("{}:{}", comment.path, comment.line),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(status_marker.to_string(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(
                        truncate(body, 60),
                        if is_deleted {
                            Style::default().fg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::White)
                        },
                    ),
                ]));

                if is_selected && !is_deleted {
                    item = item.style(base_style);
                } else if is_deleted {
                    item = item.style(base_style);
                }

                item
            })
            .collect();

        let scroll_offset = edit_state.scroll_offset;
        let visible_items: Vec<ListItem> = items.into_iter().skip(scroll_offset).take(visible_height).collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " Comments ({}/{}) ",
                edit_state.selected_comment.saturating_add(1).min(total_comments),
                total_comments
            ))
            .border_style(Style::default().fg(Color::Gray));

        let inner_area = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);

        let list = List::new(visible_items);
        frame.render_widget(list, inner_area);

        // Scrollbar
        if total_comments > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));
            let mut scrollbar_state =
                ScrollbarState::new(total_comments.saturating_sub(visible_height)).position(scroll_offset);
            frame.render_stateful_widget(
                scrollbar,
                chunks[1].inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
                &mut scrollbar_state,
            );
        }
    }

    // Status bar
    let help_text = if edit_state.showing_detail {
        "Esc/Enter/q: Close detail"
    } else if edit_state.posting {
        "Posting..."
    } else if edit_state.post_result.is_some() {
        "q: Close"
    } else {
        "j/k: Navigate | Enter: View | d: Toggle delete | e: Edit | p: Post | q: Cancel"
    };

    let status_bar = Paragraph::new(Line::from(vec![Span::styled(
        help_text,
        Style::default().fg(Color::Cyan),
    )]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(status_bar, chunks[2]);

    // Render detail modal on top if showing
    if edit_state.showing_detail {
        render_comment_detail_modal(frame, pending, edit_state);
    }
}

fn render_comment_detail_modal(
    frame: &mut Frame,
    pending: &PendingReview,
    edit_state: &PendingReviewEditState,
) {
    let Some(comment) = pending.review.comments.get(edit_state.selected_comment) else {
        return;
    };

    let body = if let Some(edited) = edit_state.edited_bodies.get(&edit_state.selected_comment) {
        edited.as_str()
    } else {
        &comment.body
    };

    let is_deleted = edit_state
        .deleted_comments
        .contains(&edit_state.selected_comment);

    let severity_str = match comment.severity {
        CommentSeverity::Critical => "Critical",
        CommentSeverity::Major => "Major",
        CommentSeverity::Minor => "Minor",
        CommentSeverity::Suggestion => "Suggestion",
    };
    let severity_color = match comment.severity {
        CommentSeverity::Critical => Color::Red,
        CommentSeverity::Major => Color::Yellow,
        CommentSeverity::Minor => Color::Blue,
        CommentSeverity::Suggestion => Color::Green,
    };

    let area = frame.area();
    let modal_width = (area.width as f32 * 0.8) as u16;
    let modal_height = (area.height as f32 * 0.7) as u16;
    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

    frame.render_widget(Clear, modal_area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::Gray)),
            Span::styled(&comment.path, Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("  Line: {}", comment.line),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![
            Span::styled("Severity: ", Style::default().fg(Color::Gray)),
            Span::styled(
                severity_str,
                Style::default()
                    .fg(severity_color)
                    .add_modifier(Modifier::BOLD),
            ),
            if is_deleted {
                Span::styled("  [DELETED]", Style::default().fg(Color::Red))
            } else if edit_state
                .edited_bodies
                .contains_key(&edit_state.selected_comment)
            {
                Span::styled("  [EDITED]", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("")
            },
        ]),
        Line::from(""),
    ];

    for text_line in body.lines() {
        lines.push(Line::from(Span::styled(
            text_line.to_string(),
            Style::default().fg(Color::White),
        )));
    }

    let title = format!(
        " Comment {}/{} ",
        edit_state.selected_comment + 1,
        pending.review.comments.len()
    );

    let content = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .title_bottom(Line::from(" Press Esc/Enter/q to close ").centered())
                .border_style(Style::default().fg(severity_color)),
        );

    frame.render_widget(content, modal_area);
}

fn truncate(s: &str, max_chars: usize) -> String {
    // Take first line only for display
    let first_line = s.lines().next().unwrap_or(s);
    let char_count = first_line.chars().count();
    if char_count <= max_chars {
        first_line.to_string()
    } else {
        let truncated: String = first_line.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}
