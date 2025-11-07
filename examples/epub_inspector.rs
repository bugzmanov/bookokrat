use epub::doc::EpubDoc;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let epub_path = if args.len() > 1 {
        &args[1]
    } else {
        "careless.epub"
    };

    println!("Opening EPUB: {epub_path}");

    let mut doc = EpubDoc::new(epub_path)?;

    println!("Title: {:?}", doc.mdata("title"));
    println!("Creator: {:?}", doc.mdata("creator"));
    println!("Total chapters: {}", doc.get_num_chapters());
    println!("\n{}\n", "=".repeat(80));

    // Extract and display chapters to find actual content - showing every 10th chapter to understand structure
    let total_chapters = doc.get_num_chapters();
    // Look at specific chapters where sections might be, plus every 5th chapter
    let mut chapters_to_show: Vec<usize> = (0..total_chapters).step_by(5).collect();
    // Add specific chapters that might contain section headers based on typical book structure
    let potential_section_chapters = [
        10, 11, 12, 26, 27, 28, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58,
        59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73,
    ];
    for &ch in &potential_section_chapters {
        if ch < total_chapters && !chapters_to_show.contains(&ch) {
            chapters_to_show.push(ch);
        }
    }
    chapters_to_show.sort();

    for &i in &chapters_to_show {
        let _ = doc.set_current_chapter(i);

        println!("CHAPTER {} RAW HTML CONTENT:", i + 1);
        println!("{}", "-".repeat(60));

        match doc.get_current_str() {
            Some((content, _mime)) => {
                // Check if this chapter contains a section header (h1 tag)
                if content.contains("<h1") {
                    println!("*** SECTION HEADER FOUND ***");
                    // Extract the h1 content
                    if let Some(start) = content.find("<h1") {
                        if let Some(end) = content[start..].find("</h1>") {
                            let h1_section = &content[start..start + end + 5];
                            println!("Section header: {h1_section}");
                        }
                    }
                }
                // Also check for h2 headers with specific classes that might be chapters
                if content.contains("h2")
                    && (content.contains("Subheader") || content.contains("bez"))
                {
                    if let Some(start) = content.find("<h2") {
                        if let Some(end) = content[start..].find("</h2>") {
                            let h2_section = &content[start..start + end + 5];
                            println!("Chapter header: {h2_section}");
                        }
                    }
                }
                // Only print first 200 chars to keep output manageable
                println!("{}", &content[..std::cmp::min(200, content.len())]);
            }
            None => {
                println!("Error reading chapter {}", i + 1);
            }
        }

        println!("\n{}\n", "=".repeat(80));
    }

    Ok(())
}
