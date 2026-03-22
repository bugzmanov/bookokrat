use clap::Parser;

#[derive(Parser)]
#[command(name = "bookokrat", version, about)]
pub struct Cli {
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
