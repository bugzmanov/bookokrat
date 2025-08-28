use bookrat::parsing::text_generator::TextGenerator;
use regex::Regex;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read the raw HTML
    let html = fs::read_to_string("debug_chapter_6_raw.html")?;
    
    // Find code blocks
    let code_block_re = Regex::new(r#"(?s)<p class="programs">(.+?)</p>"#).unwrap();
    
    println!("Step 1: Checking raw HTML for code blocks...");
    let mut block_num = 0;
    for cap in code_block_re.captures_iter(&html) {
        if let Some(content) = cap.get(1) {
            let preview = content.as_str().chars().take(100).collect::<String>();
            println!("Block {}: {} chars, preview: {}...", block_num, content.as_str().len(), preview);
            block_num += 1;
        }
    }
    println!("Total blocks found: {}", block_num);
    
    // Now simulate the clean_html_content processing
    println!("\nStep 2: Simulating clean_html_content...");
    
    // Remove style and script tags first
    let style_re = Regex::new(r"(?s)<style[^>]*>.*?</style>").unwrap();
    let script_re = Regex::new(r"(?s)<script[^>]*>.*?</script>").unwrap();
    let mut content = style_re.replace_all(&html, "").into_owned();
    content = script_re.replace_all(&content, "").into_owned();
    
    // Process code blocks with placeholders
    let mut code_blocks = Vec::new();
    
    let with_placeholders = code_block_re
        .replace_all(&content, |caps: &regex::Captures| {
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
        })
        .into_owned();
    
    println!("Code blocks stored: {}", code_blocks.len());
    
    // Check if placeholders exist
    let placeholder_count = with_placeholders.matches("<<<CODEBLOCK").count();
    println!("Placeholders in content: {}", placeholder_count);
    
    // Continue processing
    let p_tag_re = Regex::new(r"<p[^>]*>").unwrap();
    let mut text = p_tag_re.replace_all(&with_placeholders, "").to_string();
    
    text = text.replace("</p>", "\n\n");
    
    // Check what happens to the image div
    println!("\nStep 3: Checking for image tags...");
    if text.contains(r#"<div class="image">"#) {
        println!("Found image div tags");
    }
    
    // Process divs
    text = text.replace("<div>", "").replace("</div>", "\n");
    
    // Now check remaining tags
    let remaining_tags_re = Regex::new(r"<[^>]*>").unwrap();
    
    // First, let's see what tags remain
    println!("\nRemaining tags before removal:");
    for cap in remaining_tags_re.find_iter(&text) {
        let tag = cap.as_str();
        if !tag.contains("CODEBLOCK") {
            println!("  {}", tag);
        }
    }
    
    // Remove remaining tags
    text = remaining_tags_re.replace_all(&text, "").to_string();
    
    // Check if placeholders survived
    let placeholder_count_final = text.matches("<<<CODEBLOCK").count();
    println!("\nPlaceholders after all processing: {}", placeholder_count_final);
    
    // Check for >> in the text
    if text.contains(">>") {
        println!("\nWARNING: Found >> in processed text!");
        // Find context
        if let Some(pos) = text.find(">>") {
            let start = pos.saturating_sub(50);
            let end = (pos + 50).min(text.len());
            println!("Context: ...{}...", &text[start..end]);
        }
    }
    
    Ok(())
}