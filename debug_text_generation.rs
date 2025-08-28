use bookrat::parsing::text_generator::TextGenerator;
use epub::doc::EpubDoc;
use std::fs::{self, File};
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open poc.epub
    let file = File::open("poc.epub")?;
    let mut doc = EpubDoc::from_reader(BufReader::new(file))?;
    
    println!("EPUB has {} chapters", doc.get_num_pages());
    
    // Find chapter 0:2 (the one with tamperdetected code)
    for i in 0..doc.get_num_pages() {
        doc.set_current_page(i);
        
        let content = doc.get_current_str()?;
        if content.contains("tamperdetected") {
            println!("\nFound tamperdetected in chapter {} (spine index {})", i, doc.get_current_page());
            
            // Save raw HTML
            fs::write("debug_text_gen_raw.html", &content)?;
            
            // Process with TextGenerator
            let text_gen = TextGenerator::new();
            let (processed_text, title) = text_gen.process_chapter_content(&mut doc)?;
            
            println!("Chapter title: {:?}", title);
            
            // Save processed text
            fs::write("debug_text_gen_processed.txt", &processed_text)?;
            
            // Find sections with >>
            for (line_num, line) in processed_text.lines().enumerate() {
                if line.trim() == ">>" {
                    println!("Found >> at line {}", line_num + 1);
                    
                    // Show context
                    let start = line_num.saturating_sub(2);
                    let end = (line_num + 3).min(processed_text.lines().count());
                    
                    println!("Context:");
                    for i in start..end {
                        if let Some(context_line) = processed_text.lines().nth(i) {
                            println!("  {}: {}", i + 1, context_line);
                        }
                    }
                    println!();
                }
            }
            
            break;
        }
    }
    
    Ok(())
}