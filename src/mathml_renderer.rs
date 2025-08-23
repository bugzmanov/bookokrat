/*!
MathML to ASCII converter for terminal rendering.
Parses MathML expressions and generates properly positioned ASCII art.
*/

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

/// Error types for MathML processing
#[derive(Debug)]
pub enum MathMLError {
    XmlParse(String),
    InvalidStructure(String),
    UnsupportedElement(String),
}

impl std::fmt::Display for MathMLError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::XmlParse(msg) => write!(f, "XML parsing error: {msg}"),
            Self::InvalidStructure(msg) => write!(f, "Invalid MathML structure: {msg}"),
            Self::UnsupportedElement(msg) => write!(f, "Unsupported element: {msg}"),
        }
    }
}

impl std::error::Error for MathMLError {}

/// Unicode subscript character mappings
static UNICODE_SUBSCRIPTS: Lazy<HashMap<char, char>> = Lazy::new(|| {
    [
        ('0', '₀'),
        ('1', '₁'),
        ('2', '₂'),
        ('3', '₃'),
        ('4', '₄'),
        ('5', '₅'),
        ('6', '₆'),
        ('7', '₇'),
        ('8', '₈'),
        ('9', '₉'),
        ('a', 'ₐ'),
        ('e', 'ₑ'),
        ('i', 'ᵢ'),
        ('o', 'ₒ'),
        ('u', 'ᵤ'),
        ('x', 'ₓ'),
        ('h', 'ₕ'),
        ('k', 'ₖ'),
        ('l', 'ₗ'),
        ('m', 'ₘ'),
        ('n', 'ₙ'),
        ('p', 'ₚ'),
        ('r', 'ᵣ'),
        ('s', 'ₛ'),
        ('t', 'ₜ'),
        ('v', 'ᵥ'),
        ('+', '₊'),
        ('-', '₋'),
        ('=', '₌'),
        ('(', '₍'),
        (')', '₎'),
        ('ə', 'ₔ'),
    ]
    .iter()
    .copied()
    .collect()
});

/// Unicode superscript character mappings
static UNICODE_SUPERSCRIPTS: Lazy<HashMap<char, char>> = Lazy::new(|| {
    [
        ('0', '⁰'),
        ('1', '¹'),
        ('2', '²'),
        ('3', '³'),
        ('4', '⁴'),
        ('5', '⁵'),
        ('6', '⁶'),
        ('7', '⁷'),
        ('8', '⁸'),
        ('9', '⁹'),
        ('a', 'ᵃ'),
        ('b', 'ᵇ'),
        ('c', 'ᶜ'),
        ('d', 'ᵈ'),
        ('e', 'ᵉ'),
        ('f', 'ᶠ'),
        ('g', 'ᵍ'),
        ('h', 'ʰ'),
        ('i', 'ⁱ'),
        ('j', 'ʲ'),
        ('k', 'ᵏ'),
        ('l', 'ˡ'),
        ('m', 'ᵐ'),
        ('n', 'ⁿ'),
        ('o', 'ᵒ'),
        ('p', 'ᵖ'),
        ('r', 'ʳ'),
        ('s', 'ˢ'),
        ('t', 'ᵗ'),
        ('u', 'ᵘ'),
        ('v', 'ᵛ'),
        ('w', 'ʷ'),
        ('x', 'ˣ'),
        ('y', 'ʸ'),
        ('z', 'ᶻ'),
        ('A', 'ᴬ'),
        ('B', 'ᴮ'),
        ('D', 'ᴰ'),
        ('E', 'ᴱ'),
        ('G', 'ᴳ'),
        ('H', 'ᴴ'),
        ('I', 'ᴵ'),
        ('J', 'ᴶ'),
        ('K', 'ᴷ'),
        ('L', 'ᴸ'),
        ('M', 'ᴹ'),
        ('N', 'ᴺ'),
        ('O', 'ᴼ'),
        ('P', 'ᴾ'),
        ('R', 'ᴿ'),
        ('T', 'ᵀ'),
        ('U', 'ᵁ'),
        ('V', 'ⱽ'),
        ('W', 'ᵂ'),
        ('+', '⁺'),
        ('-', '⁻'),
        ('=', '⁼'),
        ('(', '⁽'),
        (')', '⁾'),
        ('\'', '′'), // Prime symbol
    ]
    .iter()
    .copied()
    .collect()
});

/// Represents a rendered math element with its dimensions and content
#[derive(Debug, Clone)]
pub struct MathBox {
    width: usize,
    height: usize,
    baseline: usize,         // Distance from top to baseline
    content: Vec<Vec<char>>, // 2D grid of characters
}

impl MathBox {
    /// Initialize a simple text box
    pub fn new(text: &str) -> Self {
        let width = text.chars().count();
        let height = if width > 0 { 1 } else { 0 };
        let baseline = 0;
        let content = if width > 0 {
            vec![text.chars().collect()]
        } else {
            vec![]
        };

        Self {
            width,
            height,
            baseline,
            content,
        }
    }

    /// Create an empty box with given dimensions
    pub fn create_empty(width: usize, height: usize, baseline: usize) -> Self {
        let content = vec![vec![' '; width]; height];
        Self {
            width,
            height,
            baseline,
            content,
        }
    }

    /// Get character at position, return space if out of bounds
    pub fn get_char(&self, x: usize, y: usize) -> char {
        if y < self.height && x < self.width {
            self.content[y][x]
        } else {
            ' '
        }
    }

    /// Set character at position
    pub fn set_char(&mut self, x: usize, y: usize, ch: char) {
        if y < self.height && x < self.width {
            self.content[y][x] = ch;
        }
    }

    /// Render the box as a string
    pub fn render(&self) -> String {
        self.content
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Parser for MathML expressions
pub struct MathMLParser {
    use_unicode: bool,
}

impl MathMLParser {
    pub fn new(use_unicode: bool) -> Self {
        Self { use_unicode }
    }

    /// Parse MathML string and return rendered ASCII box
    pub fn parse(&self, mathml: &str) -> Result<MathBox, MathMLError> {
        let mathml = mathml.trim();

        // Wrap in math tags if not present
        let mathml = if !mathml.starts_with("<math") {
            format!(r#"<math xmlns="http://www.w3.org/1998/Math/MathML">{mathml}</math>"#)
        } else {
            mathml.to_string()
        };

        // Parse XML
        let doc = roxmltree::Document::parse(&mathml)
            .map_err(|e| MathMLError::XmlParse(e.to_string()))?;

        let root = doc.root_element();
        self.process_element(&root)
    }

    /// Process a MathML element and return its rendered box
    fn process_element(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        if !node.is_element() {
            return Ok(MathBox::new(node.text().unwrap_or("")));
        }

        let tag = node.tag_name().name();

        // Match Python's approach exactly
        match tag {
            "math" => {
                // Process the content of math element
                let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
                if children.len() > 0 {
                    self.process_element(&children[0])
                } else {
                    Ok(MathBox::new(node.text().unwrap_or("")))
                }
            }

            "mrow" => {
                // Horizontal group - exactly like Python
                self.process_mrow_python_style(node)
            }

            "mi" => {
                // Identifier (variable)
                Ok(MathBox::new(node.text().unwrap_or("")))
            }

            "mo" => {
                // Operator
                let text = node.text().unwrap_or("");
                // Add spacing around binary operators
                if matches!(text, "=" | "+" | "-" | "*" | "/") {
                    Ok(MathBox::new(&format!(" {} ", text)))
                } else if matches!(text, "(" | ")" | "[" | "]" | "{" | "}") {
                    // No extra spacing for brackets, parentheses
                    Ok(MathBox::new(text))
                } else {
                    // Summation and other special operators
                    Ok(MathBox::new(text))
                }
            }

            "mn" => {
                // Number
                Ok(MathBox::new(node.text().unwrap_or("")))
            }

            "mtext" => {
                // Text
                Ok(MathBox::new(node.text().unwrap_or("")))
            }

            "mfrac" => {
                // Fraction
                self.process_fraction(node)
            }

            "msub" => {
                // Subscript
                self.process_subscript(node)
            }

            "msup" => {
                // Superscript
                self.process_superscript(node)
            }

            "munder" => {
                // Under (like sum with subscript)
                self.process_under(node)
            }

            _ => {
                // Default: concatenate children horizontally (like Python)
                let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
                if children.len() > 0 {
                    let boxes: Result<Vec<_>, _> = children
                        .iter()
                        .map(|child| self.process_element(child))
                        .collect();
                    let boxes = boxes?;
                    Ok(self.horizontal_concat(boxes))
                } else {
                    Ok(MathBox::new(node.text().unwrap_or("")))
                }
            }
        }
    }

    /// Process an mrow (horizontal group) element - matching Python exactly
    fn process_mrow_python_style(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let mut boxes = Vec::new();

        // Process text before first child
        if let Some(text) = node.text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                boxes.push(MathBox::new(trimmed));
            }
        }

        // Process children
        for child in node.children().filter(|n| n.is_element()) {
            let child_box = self.process_element(&child)?;
            if child_box.width > 0 {
                // Only add non-empty boxes
                boxes.push(child_box);
            }
            // Process text after each child (tail)
            if let Some(tail) = child.tail() {
                let trimmed = tail.trim();
                if !trimmed.is_empty() {
                    boxes.push(MathBox::new(trimmed));
                }
            }
        }

        if boxes.is_empty() {
            Ok(MathBox::new(""))
        } else {
            Ok(self.horizontal_concat(boxes))
        }
    }

    /// Process a fraction element
    fn process_fraction(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 2 {
            return Err(MathMLError::InvalidStructure(
                "Fraction needs exactly 2 children".into(),
            ));
        }

        let numerator = self.process_element(&children[0])?;
        let denominator = self.process_element(&children[1])?;

        // Calculate dimensions
        let width = numerator.width.max(denominator.width);
        let height = numerator.height + 1 + denominator.height;
        let baseline = numerator.height; // Fraction bar at baseline

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place numerator (centered, above fraction bar)
        let num_offset = (width.saturating_sub(numerator.width)) / 2;
        for y in 0..numerator.height {
            for x in 0..numerator.width {
                result.set_char(x + num_offset, y, numerator.get_char(x, y));
            }
        }

        // Draw fraction bar at baseline
        for x in 0..width {
            result.set_char(x, baseline, '─');
        }

        // Place denominator (centered, below fraction bar)
        let den_offset = (width.saturating_sub(denominator.width)) / 2;
        for y in 0..denominator.height {
            for x in 0..denominator.width {
                result.set_char(x + den_offset, baseline + 1 + y, denominator.get_char(x, y));
            }
        }

        Ok(result)
    }

    /// Try to convert text to Unicode subscripts, return None if not possible
    fn try_unicode_subscript(&self, text: &str) -> Option<String> {
        if !self.use_unicode || text.is_empty() {
            return None;
        }

        text.chars()
            .map(|c| UNICODE_SUBSCRIPTS.get(&c).copied())
            .collect::<Option<String>>()
    }

    /// Try to convert text to Unicode superscripts, return None if not possible
    fn try_unicode_superscript(&self, text: &str) -> Option<String> {
        if !self.use_unicode || text.is_empty() {
            return None;
        }

        // Special heuristic: for single-character common notation, use inline
        if text.len() == 1 && matches!(text.chars().next(), Some('\'') | Some('"') | Some('*')) {
            return Some(text.to_string());
        }

        text.chars()
            .map(|c| UNICODE_SUPERSCRIPTS.get(&c).copied())
            .collect::<Option<String>>()
    }

    /// Process a subscript element
    fn process_subscript(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 2 {
            return Err(MathMLError::InvalidStructure(
                "Subscript needs exactly 2 children".into(),
            ));
        }

        let base = self.process_element(&children[0])?;
        let subscript = self.process_element(&children[1])?;

        // Try Unicode subscript first if both base and subscript are simple text
        if base.height == 1
            && base.baseline == 0
            && subscript.height == 1
            && subscript.baseline == 0
        {
            let subscript_text = subscript.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            if let Some(unicode_sub) = self.try_unicode_subscript(&subscript_text) {
                let base_text = base.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                let combined_text = format!("{base_text}{unicode_sub}");
                return Ok(MathBox::new(&combined_text));
            }
        }

        // Fall back to multiline positioning (match Python logic exactly)
        let width = base.width + subscript.width;
        let height = base.height.max(base.baseline + 1 + subscript.height);
        let baseline = base.baseline;

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place base
        for y in 0..base.height {
            for x in 0..base.width {
                result.set_char(x, y, base.get_char(x, y));
            }
        }

        // Place subscript (below and to the right) - exactly like Python
        let sub_y_offset = base.baseline + 1;
        for y in 0..subscript.height {
            for x in 0..subscript.width {
                if sub_y_offset + y < height {
                    result.set_char(base.width + x, sub_y_offset + y, subscript.get_char(x, y));
                }
            }
        }

        Ok(result)
    }

    /// Process a superscript element
    fn process_superscript(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 2 {
            return Err(MathMLError::InvalidStructure(
                "Superscript needs exactly 2 children".into(),
            ));
        }

        let base = self.process_element(&children[0])?;
        let superscript = self.process_element(&children[1])?;

        // Try Unicode superscript first if both base and superscript are simple text
        if base.height == 1
            && base.baseline == 0
            && superscript.height == 1
            && superscript.baseline == 0
        {
            let superscript_text = superscript.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            if let Some(unicode_sup) = self.try_unicode_superscript(&superscript_text) {
                let base_text = base.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                let combined_text = format!("{base_text}{unicode_sup}");
                return Ok(MathBox::new(&combined_text));
            }
        }

        // Fall back to multiline positioning
        let width = base.width + superscript.width;
        let height = superscript.height + base.height;
        let baseline = superscript.height + base.baseline;

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place superscript (above and to the right of base)
        for y in 0..superscript.height {
            for x in 0..superscript.width {
                result.set_char(base.width + x, y, superscript.get_char(x, y));
            }
        }

        // Place base (below superscript)
        for y in 0..base.height {
            for x in 0..base.width {
                result.set_char(x, superscript.height + y, base.get_char(x, y));
            }
        }

        Ok(result)
    }

    /// Process an under element (like summation with subscript)
    fn process_under(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 2 {
            return Err(MathMLError::InvalidStructure(
                "Under element needs exactly 2 children".into(),
            ));
        }

        let base = self.process_element(&children[0])?;
        let under = self.process_element(&children[1])?;

        // Check if this is a summation - if so, add some spacing
        let is_summation = node
            .children()
            .find(|n| n.is_element())
            .and_then(|n| n.text())
            .map_or(false, |text| text.contains('∑'));

        // Calculate dimensions
        let mut width = base.width.max(under.width);
        if is_summation {
            width = width.max(2); // Ensure minimum width for summation
        }
        let height = base.height + under.height;
        let baseline = base.baseline;

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place base (centered if needed)
        let base_offset = (width.saturating_sub(base.width)) / 2;
        for y in 0..base.height {
            for x in 0..base.width {
                result.set_char(x + base_offset, y, base.get_char(x, y));
            }
        }

        // Place under (centered below)
        let under_offset = (width.saturating_sub(under.width)) / 2;
        for y in 0..under.height {
            for x in 0..under.width {
                result.set_char(x + under_offset, base.height + y, under.get_char(x, y));
            }
        }

        Ok(result)
    }

    /// Concatenate boxes horizontally, aligning at baseline
    fn horizontal_concat(&self, boxes: Vec<MathBox>) -> MathBox {
        // Filter out empty boxes
        let boxes: Vec<_> = boxes
            .into_iter()
            .filter(|b| b.width > 0 && b.height > 0)
            .collect();

        if boxes.is_empty() {
            return MathBox::new("");
        }

        if boxes.len() == 1 {
            return boxes.into_iter().next().unwrap();
        }

        // ALWAYS try to create simple single-line text first
        // This is more aggressive - if all content can fit on one line, force it
        let can_be_single_line = boxes.iter().all(|b| {
            b.height <= 1
                || (b.height == 1 && b.content.iter().all(|row| row.iter().all(|c| *c != '\n')))
        });

        if can_be_single_line {
            let combined_text: String = boxes
                .iter()
                .flat_map(|b| {
                    if b.height == 0 {
                        vec![]
                    } else if b.height == 1 {
                        vec![b.content[0].iter().collect::<String>()]
                    } else {
                        // For multi-line boxes that contain simple content, just take first line
                        vec![
                            b.content
                                .get(0)
                                .map(|row| row.iter().collect::<String>())
                                .unwrap_or_default(),
                        ]
                    }
                })
                .collect();
            return MathBox::new(&combined_text);
        }

        // Calculate dimensions (match Python exactly)
        let width = boxes.iter().map(|b| b.width).sum();
        let max_above = boxes.iter().map(|b| b.baseline).max().unwrap_or(0);
        let max_below = boxes
            .iter()
            .map(|b| b.height - b.baseline)
            .max()
            .unwrap_or(0);
        let height = max_above + max_below;
        let baseline = max_above;

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place each box (match Python exactly)
        let mut x_offset = 0;
        for b in boxes {
            let y_offset = baseline - b.baseline; // Python: y_offset = baseline - box.baseline
            for y in 0..b.height {
                for x in 0..b.width {
                    let ch = b.get_char(x, y);
                    if ch != ' ' && ch != '\0' {
                        // Only copy non-empty chars
                        if y + y_offset < height && x + x_offset < width {
                            result.set_char(x + x_offset, y + y_offset, ch);
                        }
                    }
                }
            }
            x_offset += b.width;
        }

        result
    }
}

/// Convert HTML with MathML to ASCII representation
pub fn mathml_to_ascii(html: &str, use_unicode: bool) -> Result<String, MathMLError> {
    let parser = MathMLParser::new(use_unicode);

    // Extract MathML from HTML (with DOTALL flag for multiline)
    let math_pattern = Regex::new(r"(?s)<math[^>]*>.*?</math>").unwrap();
    let matches: Vec<_> = math_pattern.find_iter(html).collect();

    if matches.is_empty() {
        // No math elements found - return original HTML
        return Ok(html.to_string());
    }

    // For now, just process the first math element found
    // In a full implementation, we'd replace them in the HTML
    let mathml = matches[0].as_str();
    let math_box = parser.parse(mathml)?;
    Ok(math_box.render())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_fraction() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mfrac>
                <mi>a</mi>
                <mi>b</mi>
            </mfrac>
        </math>
        "#;

        let result = mathml_to_ascii(mathml, true).unwrap();
        assert!(result.contains('─'));
        assert!(result.contains('a'));
        assert!(result.contains('b'));
    }

    #[test]
    fn test_unicode_subscript() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msub>
                <mi>x</mi>
                <mn>1</mn>
            </msub>
        </math>
        "#;

        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result, "x₁");
    }

    #[test]
    fn test_unicode_superscript() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msup>
                <mi>x</mi>
                <mn>2</mn>
            </msup>
        </math>
        "#;

        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result, "x²");
    }

    #[test]
    fn test_complex_equation() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mrow>
        <msub><mi>E</mi><mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow></msub>
        <mrow><mo>[</mo><mi>x</mi><mo>]</mo></mrow>
        <mo>=</mo>
        <munder><mo>∑</mo><mi>x</mi></munder>
        <mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo><mi>x</mi>
        <mo>=</mo>
        <munder><mo>∑</mo><mi>x</mi></munder>
        <mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo><mi>x</mi>
        <mfrac>
            <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
            <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
        </mfrac>
        <mo>=</mo>
        <msub><mi>E</mi><mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow></msub>
        <mrow>
            <mo>[</mo>
            <mi>x</mi>
            <mfrac>
                <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
                <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
            </mfrac>
            <mo>]</mo>
        </mrow>
    </mrow>
</math>
        "#;

        let expected = r#"
                            P(x)          P(x)
E    [x] = ∑ P(x)x = ∑ Q(x)x──── = E    [x────]
 P(x)      x         x      Q(x)    Q(x)  Q(x) "#
            .trim();
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_simple_fraction_alignment() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mrow>
        <mi>y</mi>
        <mo>=</mo>
        <mfrac>
            <mi>a</mi>
            <mi>b</mi>
        </mfrac>
        <mo>+</mo>
        <mfrac>
            <mi>c</mi>
            <mi>d</mi>
        </mfrac>
    </mrow>
</math>
        "#;

        let expected = r#"
    a   c
y = ─ + ─
    b   d"#
            .trim();
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_super_complex_equation() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="gamma left-parenthesis v right-parenthesis equals left-bracket a 1 cosine left-parenthesis 2 pi b 1 Superscript upper T Baseline v right-parenthesis comma a 1 sine left-parenthesis 2 pi b 1 Superscript upper T Baseline v right-parenthesis comma ellipsis comma a Subscript m Baseline cosine left-parenthesis 2 pi b Subscript m Baseline Superscript upper T Baseline v right-parenthesis comma a Subscript m Baseline sine left-parenthesis 2 pi b Subscript m Baseline Superscript upper T Baseline v right-parenthesis right-bracket Superscript upper T">
  <mrow>
    <mi>γ</mi>
    <mrow>
      <mo>(</mo>
      <mi>v</mi>
      <mo>)</mo>
    </mrow>
    <mo>=</mo>
    <msup><mrow><mo>[</mo><msub><mi>a</mi> <mn>1</mn> </msub><mo form="prefix">cos</mo><mrow><mo>(</mo><mn>2</mn><mi>π</mi><msup><mrow><msub><mi>b</mi> <mn>1</mn> </msub></mrow> <mi>T</mi> </msup><mi>v</mi><mo>)</mo></mrow><mo>,</mo><msub><mi>a</mi> <mn>1</mn> </msub><mo form="prefix">sin</mo><mrow><mo>(</mo><mn>2</mn><mi>π</mi><msup><mrow><msub><mi>b</mi> <mn>1</mn> </msub></mrow> <mi>T</mi> </msup><mi>v</mi><mo>)</mo></mrow><mo>,</mo><mo>...</mo><mo>,</mo><msub><mi>a</mi> <mi>m</mi> </msub><mo form="prefix">cos</mo><mrow><mo>(</mo><mn>2</mn><mi>π</mi><msup><mrow><msub><mi>b</mi> <mi>m</mi> </msub></mrow> <mi>T</mi> </msup><mi>v</mi><mo>)</mo></mrow><mo>,</mo><msub><mi>a</mi> <mi>m</mi> </msub><mo form="prefix">sin</mo><mrow><mo>(</mo><mn>2</mn><mi>π</mi><msup><mrow><msub><mi>b</mi> <mi>m</mi> </msub></mrow> <mi>T</mi> </msup><mi>v</mi><mo>)</mo></mrow><mo>]</mo></mrow> <mi>T</mi> </msup>
  </mrow>
</math>
        "#;

        let expected = r#"
γ(v) = [a₁cos(2πb₁ᵀv),a₁sin(2πb₁ᵀv),...,aₘcos(2πbₘᵀv),aₘsin(2πbₘᵀv)]ᵀ"#
            .trim();
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_prime_superscript() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="x prime equals gamma x 1 plus left-parenthesis 1 minus gamma right-parenthesis x 2">
  <mrow>
    <msup><mrow><mi>x</mi></mrow> <mo>'</mo> </msup>
    <mo>=</mo>
    <mi>γ</mi>
    <msub><mi>x</mi> <mn>1</mn> </msub>
    <mo>+</mo>
    <mrow>
      <mo>(</mo>
      <mn>1</mn>
      <mo>-</mo>
      <mi>γ</mi>
      <mo>)</mo>
    </mrow>
    <msub><mi>x</mi> <mn>2</mn> </msub>
  </mrow>
</math>
        "#;

        let expected = r#"
x' = γx₁ + (1 - γ)x₂"#
            .trim();
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_summation_with_subscript() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <munder>
                <mo>∑</mo>
                <mi>i</mi>
            </munder>
        </math>
        "#;

        let expected = r#"∑
i"#
        .trim();
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }
}
