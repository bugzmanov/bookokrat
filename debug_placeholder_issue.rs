use std::fs;
use regex::Regex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the raw HTML content
    let html = fs::read_to_string("debug_chapter_6_raw.html")?;
    
    // Simulate the text_generator processing
    println!("Step 1: Finding code blocks...");
    
    let code_block_re = Regex::new(r#"(?s)<p class="programs">(.+?)</p>"#).unwrap();
    let matches = code_block_re.find_iter(&html).count();
    println!("Found {} code block matches", matches);
    
    // Process like text_generator does
    let mut code_blocks = Vec::new();
    let with_placeholders = code_block_re.replace_all(&html, |caps: &regex::Captures| {
        if let Some(code_content) = caps.get(1) {
            let code = code_content.as_str()
                .replace("<br/>", "\n")
                .replace("<br />", "\n")
                .replace("<br>", "\n")
                .replace("&#160;", " ")
                .replace("&gt;", ">")
                .replace("&lt;", "<")
                .replace("&#38;", "&")
                .replace("&amp;", "&");
            
            // Remove span tags
            let span_re = Regex::new(r"</?span[^>]*>").unwrap();
            let code = span_re.replace_all(&code, "");
            
            // Add indentation
            let indented_code = code.lines()
                .map(|line| {
                    if line.trim().is_empty() {
                        String::new()
                    } else {
                        format!("    {}", line)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            
            let block_index = code_blocks.len();
            println!("Storing code block {} with {} lines", block_index, indented_code.lines().count());
            code_blocks.push(format!("\n{}\n", indented_code));
            
            format!("\n<<<CODEBLOCK{}>>>\n", block_index)
        } else {
            caps[0].to_string()
        }
    }).to_string();
    
    println!("\nStep 2: After placeholder replacement, found {} code blocks", code_blocks.len());
    
    // Check if placeholders exist
    let placeholder_count = with_placeholders.matches("<<<CODEBLOCK").count();
    println!("Placeholders in content: {}", placeholder_count);
    
    // Simulate the rest of the processing
    let p_tag_re = Regex::new(r"<p[^>]*>").unwrap();
    let mut text = p_tag_re.replace_all(&with_placeholders, "").to_string();
    
    text = text
        .replace("</p>", "\n\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    
    // Remove remaining tags
    let remaining_tags_re = Regex::new(r"<[^>]*>").unwrap();
    text = remaining_tags_re.replace_all(&text, "").to_string();
    
    // Check placeholders after tag removal
    let placeholder_count_after = text.matches("<<<CODEBLOCK").count();
    println!("Placeholders after tag removal: {}", placeholder_count_after);
    
    // Now check what happens with multi_space_re
    let multi_space_re = Regex::new(r" +").unwrap();
    text = multi_space_re.replace_all(&text, " ").to_string();
    
    let placeholder_count_after_space = text.matches("<<<CODEBLOCK").count();
    println!("Placeholders after space processing: {}", placeholder_count_after_space);
    
    // Restore code blocks
    println!("\nStep 3: Restoring code blocks...");
    for (index, code_block) in code_blocks.iter().enumerate() {
        let placeholder = format!("<<<CODEBLOCK{}>>>", index);
        if text.contains(&placeholder) {
            println!("Found placeholder {} in text", index);
            text = text.replace(&placeholder, code_block);
        } else {
            println!("WARNING: Placeholder {} NOT FOUND in text!", index);
        }
    }
    
    // Check for ">>" in final text
    if text.contains(">>") {
        println!("\nWARNING: Found >> in final text!");
        // Find where
        for (i, line) in text.lines().enumerate() {
            if line.trim() == ">>" {
                println!(">> found at line {}", i + 1);
            }
        }
    }
    
    Ok(())
}