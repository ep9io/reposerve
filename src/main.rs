//! The command-line front end.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use reposerve::config::{Config, Overrides};
use reposerve::{Error, Server};

/// Serve a repository to a browser on the local network.
#[derive(Debug, Parser)]
#[command(
    name = "reposerve",
    version,
    about = "Serve a repository's Markdown, source files and notebooks to a browser",
    after_help = "A value given here wins over the configuration file, and the file wins over the default."
)]
struct Cli {
    /// The directory to serve.
    #[arg(short, long, value_name = "DIR")]
    root: Option<PathBuf>,

    /// The address to listen on. `0.0.0.0` reaches the local network.
    #[arg(short, long, value_name = "ADDRESS")]
    bind: Option<String>,

    /// The port to listen on.
    #[arg(short, long, value_name = "PORT")]
    port: Option<u16>,

    /// The name shown in the header and the browser title.
    #[arg(long, value_name = "TEXT")]
    title: Option<String>,

    /// Require this token. A first visit may pass it as `?token=...`.
    #[arg(long, value_name = "VALUE")]
    token: Option<String>,

    /// The extensions shown on a first visit, separated by commas.
    #[arg(short, long, value_name = "LIST")]
    extensions: Option<String>,

    /// Only serve these directory prefixes, separated by commas.
    #[arg(long, value_name = "LIST")]
    include: Option<String>,

    /// The colour scheme a first visit starts with: `auto`, `light` or `dark`.
    #[arg(long, value_name = "MODE")]
    theme: Option<String>,

    /// The width of the reading column, in `rem`.
    #[arg(long, value_name = "REM")]
    reading_width: Option<u16>,

    /// Do not render math.
    #[arg(long)]
    no_math: bool,

    /// Do not show the table of contents.
    #[arg(long)]
    no_toc: bool,

    /// Show entries whose name starts with a dot.
    #[arg(long)]
    hidden: bool,

    /// Open a browser once the server is listening.
    #[arg(long)]
    open: bool,

    /// Read settings from this TOML file.
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Do not reload a page when a file changes.
    #[arg(long)]
    no_live_reload: bool,

    /// Print the resolved configuration and exit.
    #[arg(long)]
    show_config: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Error> {
    let cli = Cli::parse();

    // A configuration file in the root is read when no other file is named.
    let root = cli.root.clone().unwrap_or_else(|| PathBuf::from("."));
    let base = match &cli.config {
        Some(path) => Some(Config::read_file(path)?),
        None => Config::discover(&root)?,
    };

    let config = base.unwrap_or_default().with_overrides(Overrides {
        root: cli.root,
        bind: cli.bind,
        port: cli.port,
        title: cli.title,
        token: cli.token,
        live_reload: cli.no_live_reload.then_some(false),
        extensions: cli.extensions,
        include: cli.include,
        theme: cli.theme,
        reading_width: cli.reading_width,
        math: cli.no_math.then_some(false),
        toc: cli.no_toc.then_some(false),
        hidden: cli.hidden.then_some(true),
        open: cli.open.then_some(true),
    })?;
    config.validate()?;

    if cli.show_config {
        println!("{config:#?}");
        return Ok(());
    }

    let server = Server::bind(config).await?;
    let urls = server.report();
    if server.wants_browser() {
        if let Some(url) = urls.first() {
            if let Err(error) = open::that(url) {
                eprintln!("warning: cannot open a browser: {error}");
            }
        }
    }
    server.serve().await
}
