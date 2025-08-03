fn main() {
    let html = r#"<div class="image"><img src="../images/f0018_01.jpg" alt="Image"/></div>"#;
    
    println!("Original: {}", html);
    
    // Simulate text_generator processing
    let text = html
        .replace("<div>", "")
        .replace("</div>", "\n");
    
    println!("After div replacement: '{}'", text);
    
    // Check what remains after removing all tags
    let tag_regex = regex::Regex::new(r"<[^>]*>").unwrap();
    let final_text = tag_regex.replace_all(&text, "");
    
    println!("After tag removal: '{}'", final_text);
}