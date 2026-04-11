use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bookokrat", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// EPUB or PDF file to open
    pub file: Option<String>,

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
