use clap::Parser;

#[derive(Parser)]
#[command(name = "bookokrat", version, about)]
pub struct Cli {
    /// EPUB or PDF file to open
    pub file: Option<String>,

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
