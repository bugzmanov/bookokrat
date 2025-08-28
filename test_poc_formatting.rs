use bookrat::parsing::text_generator::TextGenerator;
use epub::doc::EpubDoc;
use std::fs::File;
use std::io::BufReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open poc.epub
    let file = File::open("poc.epub")?;
    let mut doc = EpubDoc::from_reader(BufReader::new(file))?;
    
    println!("EPUB has {} chapters", doc.get_num_pages());
    
    // Find chapter 0:2 (the one with tamperdetected code)
    let mut found_chapter = false;
    for i in 0..doc.get_num_pages() {
        doc.set_current_page(i);
        
        let content = doc.get_current_str()?;
        if content.contains("tamperdetected") {
            println!("\nFound tamperdetected in chapter {} (spine index {})", i, doc.get_current_page());
            
            // Process with TextGenerator
            let text_gen = TextGenerator::new();
            let (processed_text, title) = text_gen.process_chapter_content(&mut doc)?;
            
            println!("Chapter title: {:?}", title);
            println!("\nProcessed content preview:");
            
            // Find the code block section
            if let Some(pos) = processed_text.find("tamperdetected") {
                let start = processed_text[..pos].rfind('\n').unwrap_or(0);
                let end = pos + processed_text[pos..].find("\n\n").unwrap_or(processed_text.len() - pos);
                
                println!("Code block containing 'tamperdetected':");
                println!("{}", &processed_text[start..end]);
            }
            
            found_chapter = true;
            break;
        }
    }
    
    if !found_chapter {
        println!("Could not find chapter with 'tamperdetected' code");
    }
    
    Ok(())
}