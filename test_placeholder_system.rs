use regex::Regex;

fn main() {
    // Simulate the HTML content processing
    let html = r#"<p>Before code</p>
<p class="programs">if(tamperdetected) {
    do_something();
}</p>
<p>After code</p>"#;

    println!("Original HTML:\n{}\n", html);

    // Simulate the code block regex and placeholder system
    let code_block_re = Regex::new(r#"(?s)<p class="programs">(.+?)</p>"#).unwrap();
    let mut code_blocks = Vec::new();
    
    let with_placeholders = code_block_re
        .replace_all(html, |caps: &regex::Captures| {
            if let Some(code_content) = caps.get(1) {
                let code = code_content.as_str();
                println!("Found code block: {}", code);
                
                // Add indentation
                let indented = code.lines()
                    .map(|line| format!("    {}", line))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                let block_index = code_blocks.len();
                code_blocks.push(format!("\n{}\n", indented));
                
                format!("\n<<<CODEBLOCK{}>>>\n", block_index)
            } else {
                caps[0].to_string()
            }
        })
        .to_string();
    
    println!("With placeholders:\n{}\n", with_placeholders);
    println!("Stored {} code blocks", code_blocks.len());
    
    // Now remove tags
    let tag_re = Regex::new(r"<[^>]*>").unwrap();
    let mut text = tag_re.replace_all(&with_placeholders, "").to_string();
    
    println!("After tag removal:\n{}\n", text);
    
    // Restore code blocks
    for (index, code_block) in code_blocks.iter().enumerate() {
        let placeholder = format!("<<<CODEBLOCK{}>>>", index);
        text = text.replace(&placeholder, code_block);
    }
    
    println!("Final text:\n{}", text);
}