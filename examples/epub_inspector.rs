use epub::doc::EpubDoc;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let epub_path = if args.len() > 1 {
        &args[1]
    } else {
        "careless.epub"
    };

    println!("Opening EPUB: {}", epub_path);
    
    let mut doc = EpubDoc::new(epub_path)?;

    println!("Title: {:?}", doc.mdata("title"));
    println!("Creator: {:?}", doc.mdata("creator"));
    println!("Total chapters: {}", doc.get_num_pages());
    println!("\n{}\n", "=".repeat(80));

    // Extract and display chapters 5-10 to find actual content
    let start_chapter = 5;
    let chapters_to_show = std::cmp::min(start_chapter + 5, doc.get_num_pages());
    
    for i in start_chapter..chapters_to_show {
        let _ = doc.set_current_page(i);
        
        println!("CHAPTER {} RAW HTML CONTENT:", i + 1);
        println!("{}", "-".repeat(60));
        
        match doc.get_current_str() {
            Ok(content) => {
                println!("{}", content);
            },
            Err(e) => {
                println!("Error reading chapter {}: {}", i + 1, e);
            }
        }
        
        println!("\n{}\n", "=".repeat(80));
    }

    Ok(())
}