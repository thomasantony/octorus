use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io;
use std::panic;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// Use modules from the library crate
use octorus::{app, cache, config, github, loader, syntax};

// init is only used by the binary, not needed for benchmarks
mod init;
mod post;

#[derive(Parser, Debug)]
#[command(name = "or")]
#[command(about = "TUI for GitHub PR review, designed for Helix editor users")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Repository name (e.g., "owner/repo"). Auto-detected from current directory if omitted.
    #[arg(short, long)]
    repo: Option<String>,

    /// Pull request number. Shows PR list if omitted.
    #[arg(short, long)]
    pr: Option<u32>,

    /// Start AI Rally mode directly
    #[arg(long, default_value = "false")]
    ai_rally: bool,

    /// Working directory for AI agents (default: current directory)
    #[arg(long)]
    working_dir: Option<String>,

    /// Dry-run mode: run reviewer only, save output to disk, skip GitHub posting
    #[arg(long, default_value = "false")]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize configuration files and prompt templates
    Init {
        /// Force overwrite existing files
        #[arg(long, default_value = "false")]
        force: bool,
    },
    /// Remove AI Rally session data
    Clean,
    /// Post a saved dry-run review to GitHub
    Post {
        /// Path to the pending review JSON file
        file: String,
    },
    /// View and edit a saved dry-run review in the TUI
    Review {
        /// Path to the pending review JSON file (auto-detected if omitted)
        file: Option<String>,
    },
}

/// Restore terminal to normal state
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

/// Set up panic hook to restore terminal on panic
fn setup_panic_hook() {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        restore_terminal();
        original_hook(panic_info);
    }));
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up panic hook before anything else
    setup_panic_hook();

    let args = Args::parse();

    // Handle subcommands
    if let Some(command) = args.command {
        return match command {
            Commands::Init { force } => init::run_init(force),
            Commands::Clean => {
                cache::cleanup_rally_sessions();
                let rally_dir = cache::cache_dir().join("rally");
                println!("Rally sessions cleaned: {}", rally_dir.display());
                Ok(())
            }
            Commands::Post { file } => post::run_post(&file).await,
            Commands::Review { file } => {
                let path = match file {
                    Some(f) => {
                        if !std::path::Path::new(&f).exists() {
                            eprintln!("Error: file not found: {}", f);
                            std::process::exit(1);
                        }
                        f
                    }
                    None => resolve_pending_review_path()?,
                };
                run_with_pending_review(&path).await
            }
        };
    }

    // Detect or use provided repo
    let repo = match args.repo.clone() {
        Some(r) => r,
        None => match github::detect_repo().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    };

    // Pre-initialize syntax highlighting in background to avoid delay on first diff view
    std::thread::spawn(|| {
        let _ = syntax::syntax_set();
        let _ = syntax::theme_set();
    });

    let config = config::Config::load()?;

    // Check if we have a specific PR number
    if let Some(pr) = args.pr {
        // Existing flow: open specific PR
        run_with_pr(&repo, pr, &config, &args).await
    } else {
        // New flow: show PR list
        run_with_pr_list(&repo, config, &args).await
    }
}

/// Run the app with a specific PR number (existing flow)
async fn run_with_pr(repo: &str, pr: u32, config: &config::Config, args: &Args) -> Result<()> {
    // リトライ用のチャンネル
    let (retry_tx, mut retry_rx) = mpsc::channel::<()>(1);

    // 常に Loading 状態で開始し、バックグラウンドで API 取得
    let (mut app, tx) = app::App::new_loading(repo, pr, config.clone());

    app.set_retry_sender(retry_tx);
    setup_working_dir(&mut app, args);

    // Set flag to start AI Rally mode when --ai-rally is passed
    if args.ai_rally {
        app.set_start_ai_rally_on_load(true);
    }

    if args.dry_run {
        if !args.ai_rally {
            eprintln!("Warning: --dry-run has no effect without --ai-rally");
        }
        app.set_dry_run(true);
    }

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // バックグラウンドでAPI取得
    let repo_clone = repo.to_string();
    let pr_number = pr;

    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = async {
                loader::fetch_pr_data(repo_clone.clone(), pr_number, loader::FetchMode::Fresh, tx.clone()).await;

                while retry_rx.recv().await.is_some() {
                    let tx_retry = tx.clone();
                    loader::fetch_pr_data(repo_clone.clone(), pr_number, loader::FetchMode::Fresh, tx_retry)
                        .await;
                }
            } => {}
        }
    });

    // Run the app and ensure terminal is restored on error
    let result = app.run().await;

    // Signal background tasks to stop
    cancel_token.cancel();

    if result.is_err() {
        restore_terminal();
    }

    // spawn_blocking タスク（プリフェッチ等）が巨大ファイル処理中の場合、
    // tokio ランタイムの drop が完了を待ち続けるため、即座にプロセスを終了する。
    // これにより Drop ベースのクリーンアップはスキップされるが、バックグラウンドタスクは
    // cancel_token.cancel() で明示的に停止済みであり、残るのは spawn_blocking の
    // tree-sitter パース処理のみ。OS がプロセス終了時にリソースを回収するため問題なし。
    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
}

/// Run the app with PR list (new flow)
async fn run_with_pr_list(repo: &str, config: config::Config, args: &Args) -> Result<()> {
    let mut app = app::App::new_pr_list(repo, config);
    setup_working_dir(&mut app, args);

    // Set pending AI Rally flag if --ai-rally was passed
    if args.ai_rally {
        app.set_pending_ai_rally(true);
    }

    if args.dry_run {
        if !args.ai_rally {
            eprintln!("Warning: --dry-run has no effect without --ai-rally");
        }
        app.set_dry_run(true);
    }

    // Start loading PR list
    let (tx, rx) = mpsc::channel(2);
    app.set_pr_list_receiver(rx);

    let repo_clone = repo.to_string();
    let state_filter = app.pr_list_state_filter;

    tokio::spawn(async move {
        let result = github::fetch_pr_list(&repo_clone, state_filter, 30).await;
        let _ = tx.send(result.map_err(|e| e.to_string())).await;
    });

    // Run the app
    let result = app.run().await;

    if result.is_err() {
        restore_terminal();
    }

    // run_with_pr と同様、spawn_blocking タスクの完了待ちによるハングを防止するため
    // 即座にプロセスを終了する。バックグラウンドタスクやサブプロセスの明示的な停止は
    // app.run() 内で完了済み。
    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
}

/// Find a pending review file automatically.
/// If there's exactly one, confirm with the user. If multiple, let them pick.
fn resolve_pending_review_path() -> Result<String> {
    use octorus::ai::pending_review::find_pending_reviews;

    let reviews = find_pending_reviews();

    if reviews.is_empty() {
        eprintln!("No pending reviews found.");
        eprintln!("Run AI Rally with --dry-run first, or specify a file path: or review <file>");
        std::process::exit(1);
    }

    if reviews.len() == 1 {
        let r = &reviews[0];
        eprintln!(
            "Found pending review: {} #{} ({} comments, {})",
            r.repo, r.pr_number, r.comment_count, r.created_at
        );
        eprint!("Open this review? [Y/n] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();
        if !input.is_empty() && !input.eq_ignore_ascii_case("y") {
            eprintln!("Cancelled.");
            std::process::exit(0);
        }
        return Ok(r.path.to_string_lossy().to_string());
    }

    // Multiple reviews found
    eprintln!("Found {} pending reviews:\n", reviews.len());
    for (i, r) in reviews.iter().enumerate() {
        eprintln!(
            "  [{}] {} #{} — {} comments ({})",
            i + 1,
            r.repo,
            r.pr_number,
            r.comment_count,
            r.created_at
        );
    }
    eprintln!();
    eprint!("Select a review [1-{}]: ", reviews.len());
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let choice: usize = input
        .trim()
        .parse()
        .ok()
        .filter(|&n| n >= 1 && n <= reviews.len())
        .unwrap_or_else(|| {
            eprintln!("Invalid selection.");
            std::process::exit(1);
        });

    Ok(reviews[choice - 1].path.to_string_lossy().to_string())
}

/// Run the app in standalone pending-review mode (from a saved dry-run file)
async fn run_with_pending_review(file_path: &str) -> Result<()> {
    use octorus::ai::pending_review::read_pending_review;
    use std::path::Path;

    let pending = read_pending_review(Path::new(file_path))?;
    let repo = pending.repo.clone();
    let pr_number = pending.pr_number;

    let config = config::Config::load()?;

    // Create app with new_loading so PR data (files/patches) is fetched in
    // background — this powers the code-context display in the detail modal.
    let (mut app, tx) = app::App::new_loading(&repo, pr_number, config);

    // Transition directly to PendingReviewEdit state
    app.pending_review = Some(pending);
    app.pending_review_edit = Some(app::PendingReviewEditState {
        selected_comment: 0,
        deleted_comments: std::collections::HashSet::new(),
        edited_bodies: std::collections::HashMap::new(),
        scroll_offset: 0,
        posting: false,
        post_result: None,
        showing_detail: false,
        showing_summary: false,
        include_header: true,
        post_summary: true,
    });
    app.state = app::AppState::PendingReviewEdit;
    app.set_standalone_pending_review(true);

    // Cancellation token for graceful shutdown
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // Fetch PR data in background (for code context in detail modal)
    let repo_clone = repo.clone();
    tokio::spawn(async move {
        tokio::select! {
            _ = token_clone.cancelled() => {}
            _ = loader::fetch_pr_data(repo_clone, pr_number, loader::FetchMode::Fresh, tx) => {}
        }
    });

    let result = app.run().await;
    cancel_token.cancel();

    if result.is_err() {
        restore_terminal();
    }

    let exit_code = if result.is_ok() { 0 } else { 1 };
    std::process::exit(exit_code);
}

/// Set up working directory for AI agents
fn setup_working_dir(app: &mut app::App, args: &Args) {
    if let Some(dir) = args.working_dir.clone() {
        app.set_working_dir(Some(dir));
    } else {
        // Use current directory as default.
        // Note: current_dir() can fail in edge cases (e.g., if the current directory
        // has been deleted, or on some restricted environments). When --ai-rally is
        // used without --working-dir, we need a valid directory for the AI agents.
        match std::env::current_dir() {
            Ok(cwd) => {
                app.set_working_dir(Some(cwd.to_string_lossy().to_string()));
            }
            Err(e) => {
                if args.ai_rally {
                    eprintln!(
                        "Warning: Failed to get current directory: {}. AI Rally may not work correctly without --working-dir.",
                        e
                    );
                }
                // Continue without setting working_dir; it's optional for non-AI-Rally usage
            }
        }
    }
}
