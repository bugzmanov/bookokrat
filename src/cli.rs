use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bookokrat", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// EPUB or PDF file to open
    pub file: Option<String>,

    /// Directory to use as the library (defaults to current directory)
    #[arg(long, short = 'd', conflicts_with = "file")]
    pub directory: Option<String>,

    /// Open at a given chapter
    #[arg(long, requires = "file", conflicts_with = "page")]
    pub chapter: Option<usize>,

    /// Open at a given page
    #[arg(long, requires = "file", conflicts_with = "chapter")]
    pub page: Option<usize>,

    /// Hide sidebar, show content only
    #[arg(long)]
    pub zen_mode: bool,

    /// Open the most recently read book across all libraries
    #[arg(long, short = 'c')]
    pub continue_reading: bool,

    /// Disable persistence and auto-loading
    #[arg(long)]
    pub test_mode: bool,

    /// SyncTeX forward search: send LINE:COLUMN:FILE to a running instance
    #[cfg(feature = "pdf")]
    #[arg(long)]
    pub synctex_forward: Option<String>,

    /// Print the default keybindings as flat TOML (one binding per line) and exit.
    #[arg(long)]
    pub print_default_keybindings: bool,

    /// Print the default keybindings as grouped TOML ([context] sections) and exit.
    #[arg(long)]
    pub print_default_keybindings_grouped: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print book content, TOC, or metadata to stdout
    Print {
        /// EPUB or PDF file
        file: String,

        /// Print table of contents
        #[arg(long)]
        toc: bool,

        /// Print book metadata
        #[arg(long)]
        info: bool,

        /// Chapter number to print, 1-indexed
        #[arg(long, conflicts_with = "pages")]
        chapter: Option<usize>,

        /// Page number to print, 1-indexed
        #[arg(long, conflicts_with = "chapter")]
        pages: Option<usize>,
    },
}
