use bookrat::parsing::text_generator::TextGenerator;
use epub::doc::EpubDoc;
use std::fs::{self, File};
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open poc.epub
    let file = File::open("poc.epub")?;
    let mut doc = EpubDoc::from_reader(BufReader::new(file))?;
    
    println!("EPUB has {} chapters", doc.get_num_pages());
    
    // Find the chapter with tamperdetected
    for i in 0..doc.get_num_pages() {
        doc.set_current_page(i);
        
        let content = doc.get_current_str()?;
        if content.contains("tamperdetected") {
            println!("\nFound tamperdetected in chapter {}", i);
            
            // Process with TextGenerator
            let text_gen = TextGenerator::new();
            let (processed_text, title) = text_gen.process_chapter_content(&mut doc)?;
            
            println!("Chapter title: {:?}", title);
            
            // Check for >> in the processed text
            let double_arrow_count = processed_text.matches(">>").count();
            println!("Found {} instances of '>>' in processed text", double_arrow_count);
            
            // Check for code blocks
            let code_lines = processed_text.lines()
                .filter(|line| line.starts_with("    ") && line.trim().len() > 0)
                .count();
            println!("Found {} code lines (4-space indented)", code_lines);
            
            // Save the current processed output
            fs::write("verify_current_processed.txt", &processed_text)?;
            println!("\nSaved current processed output to verify_current_processed.txt");
            
            // Show a sample around tamperdetected
            if let Some(pos) = processed_text.find("tamperdetected") {
                let start = processed_text[..pos].rfind('\n').unwrap_or(0);
                let end = pos + 200.min(processed_text.len() - pos);
                println!("\nSample around tamperdetected:");
                println!("{}", &processed_text[start..end]);
            }
            
            break;
        }
    }
    
    Ok(())
}