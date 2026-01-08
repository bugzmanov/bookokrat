use std::collections::HashMap;

pub struct TemplateEngine;

impl TemplateEngine {
    /// Render a template string by replacing {{variable}} placeholders with actual values
    pub fn render(template: &str, variables: &HashMap<String, String>) -> String {
        let mut result = template.to_string();

        for (key, value) in variables {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_rendering() {
        let template = "Title: {{book_title}}\nDate: {{export_date}}";
        let mut vars = HashMap::new();
        vars.insert("book_title".to_string(), "Test Book".to_string());
        vars.insert("export_date".to_string(), "2024-01-01".to_string());

        let result = TemplateEngine::render(template, &vars);
        assert_eq!(result, "Title: Test Book\nDate: 2024-01-01");
    }

    #[test]
    fn test_multiple_same_variable() {
        let template = "{{name}} is {{name}}";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Bob".to_string());

        let result = TemplateEngine::render(template, &vars);
        assert_eq!(result, "Bob is Bob");
    }

    #[test]
    fn test_missing_variable() {
        let template = "Title: {{book_title}}\nMissing: {{unknown}}";
        let mut vars = HashMap::new();
        vars.insert("book_title".to_string(), "Test Book".to_string());

        let result = TemplateEngine::render(template, &vars);
        assert_eq!(result, "Title: Test Book\nMissing: {{unknown}}");
    }

    #[test]
    fn test_empty_template() {
        let template = "";
        let vars = HashMap::new();

        let result = TemplateEngine::render(template, &vars);
        assert_eq!(result, "");
    }

    #[test]
    fn test_no_variables() {
        let template = "Static text with no variables";
        let vars = HashMap::new();

        let result = TemplateEngine::render(template, &vars);
        assert_eq!(result, "Static text with no variables");
    }
}
