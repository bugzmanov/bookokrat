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
}

impl std::fmt::Display for MathMLError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::XmlParse(msg) => write!(f, "XML parsing error: {msg}"),
            Self::InvalidStructure(msg) => write!(f, "Invalid MathML structure: {msg}"),
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
        ('j', 'ⱼ'),
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
        (',', ','), // Comma stays as-is
        (' ', ' '), // Space stays as-is
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
        ('θ', 'ᶿ'),
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

    /// Try to convert text to Unicode subscripts (public associated function)
    pub fn try_unicode_subscript(text: &str, use_unicode: bool) -> Option<String> {
        if !use_unicode || text.is_empty() {
            return None;
        }

        text.chars()
            .map(|c| {
                // Handle spaces and commas in subscripts (they stay as-is)
                if c == ' ' || c == ',' {
                    Some(c)
                } else {
                    UNICODE_SUBSCRIPTS.get(&c).copied()
                }
            })
            .collect::<Option<String>>()
    }

    /// Try to convert text to Unicode superscripts (public associated function)
    pub fn try_unicode_superscript(text: &str, use_unicode: bool) -> Option<String> {
        if !use_unicode || text.is_empty() {
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
                if !children.is_empty() {
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
                // Check if it's a prefix operator (like log)
                let form = node.attribute("form").unwrap_or("");

                // Handle special operators
                if form == "prefix" || matches!(text, "log" | "ln" | "sin" | "cos" | "tan" | "exp")
                {
                    // Prefix operators don't get extra spacing
                    Ok(MathBox::new(text))
                } else if matches!(text, "=" | "+" | "-" | "*" | "/" | "≠") {
                    // Add spacing around binary operators
                    Ok(MathBox::new(&format!(" {text} ")))
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

            "mspace" => {
                // Space - render as a single space character for terminal
                // In MathML, mspace can have width/height attributes but for terminal
                // rendering we'll just use a simple space
                Ok(MathBox::new(" "))
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

            "msubsup" => {
                // Subscript and superscript
                self.process_subscript_superscript(node)
            }

            "munder" => {
                // Under (like sum with subscript)
                self.process_under(node)
            }

            "munderover" => {
                // Under and over (like sum with both sub and superscript)
                self.process_underover(node)
            }

            "msqrt" => {
                // Square root
                self.process_square_root(node)
            }

            "mroot" => {
                // Nth root
                self.process_nth_root(node)
            }

            "mtable" => {
                // Table
                self.process_table(node)
            }

            "mtr" => {
                // Table row
                self.process_table_row(node)
            }

            "mtd" => {
                // Table data/cell
                self.process_table_cell(node)
            }

            "mfenced" => {
                // Fenced expression (with braces, brackets, etc.)
                self.process_fenced(node)
            }

            _ => {
                // Default: concatenate children horizontally (like Python)
                let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
                if !children.is_empty() {
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
        let mut prev_child_tag = None;
        for child in node.children().filter(|n| n.is_element()) {
            // Get the tag name
            let child_tag = child.tag_name().name();

            // Add spacing between fraction and summation
            if prev_child_tag == Some("mfrac")
                && matches!(child_tag, "msubsup" | "munderover" | "munder" | "mover")
            {
                boxes.push(MathBox::new("  ")); // Add two spaces
            }

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

            prev_child_tag = Some(child_tag);
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

        // Check for invisible fraction (linethickness="0pt") - used for conditions in summations
        let linethickness = node.attribute("linethickness").unwrap_or("");
        let is_invisible = linethickness == "0pt";

        let numerator = self.process_element(&children[0])?;
        let denominator = self.process_element(&children[1])?;

        // For invisible fractions, stack vertically without a line
        if is_invisible {
            let width = numerator.width.max(denominator.width);
            let height = numerator.height + denominator.height;
            let baseline = numerator.height - 1; // Adjust baseline

            // Create result box
            let mut result = MathBox::create_empty(width, height, baseline);

            // Place numerator (centered, top)
            let num_offset = (width.saturating_sub(numerator.width)) / 2;
            for y in 0..numerator.height {
                for x in 0..numerator.width {
                    result.set_char(x + num_offset, y, numerator.get_char(x, y));
                }
            }

            // Place denominator directly below (no bar)
            let den_offset = (width.saturating_sub(denominator.width)) / 2;
            for y in 0..denominator.height {
                for x in 0..denominator.width {
                    result.set_char(
                        x + den_offset,
                        numerator.height + y,
                        denominator.get_char(x, y),
                    );
                }
            }

            return Ok(result);
        }

        // Regular fraction with visible bar
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

        // Try Unicode subscript first if both base and subscript are simple text or can be flattened to single line
        if base.height == 1 && base.baseline == 0 {
            // Get the subscript text, handling both single-line and multi-line cases
            let subscript_text = if subscript.height == 1 && subscript.baseline == 0 {
                // Simple single-line case
                subscript.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                // For multi-line subscripts, try to flatten them to a single line
                // This handles cases like mrow with "i - 1" that create multi-line boxes
                let flattened = subscript
                    .content
                    .iter()
                    .map(|row| row.iter().collect::<String>().trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<String>>()
                    .join("")
                    .trim()
                    .to_string();

                // Only use flattened text if it's reasonably short (likely a simple subscript)
                // Allow up to 20 characters for things like "left/right"
                if flattened.len() <= 20 {
                    flattened
                } else {
                    String::new() // Fall back to multiline rendering
                }
            };

            // Try Unicode subscript if we have reasonable text
            if !subscript_text.is_empty() {
                if let Some(unicode_sub) =
                    Self::try_unicode_subscript(&subscript_text, self.use_unicode)
                {
                    let base_text = base.content[0]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string();
                    let combined_text = format!("{base_text}{unicode_sub}");
                    return Ok(MathBox::new(&combined_text));
                }

                // Try LaTeX notation for subscripts when Unicode fails
                // Allow longer text and more characters like '/' for expressions like "left/right"
                if subscript_text.len() <= 20
                    && subscript_text.chars().all(|c| {
                        c.is_alphanumeric() || c == '-' || c == '+' || c == '/' || c == ' '
                    })
                {
                    let base_text = base.content[0]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string();
                    let latex_text = format!("{base_text}_{subscript_text}");
                    return Ok(MathBox::new(&latex_text));
                }
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
            if let Some(unicode_sup) =
                Self::try_unicode_superscript(&superscript_text, self.use_unicode)
            {
                let base_text = base.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                let combined_text = format!("{base_text}{unicode_sup}");
                return Ok(MathBox::new(&combined_text));
            }

            // Try LaTeX notation for simple superscripts (1-2 characters) when Unicode fails
            if superscript_text.len() <= 2 && superscript_text.chars().all(|c| c.is_alphanumeric())
            {
                let base_text = base.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                let latex_text = format!("{base_text}^{superscript_text}");
                return Ok(MathBox::new(&latex_text));
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

    /// Process a subscript and superscript element (msubsup)
    fn process_subscript_superscript(
        &self,
        node: &roxmltree::Node,
    ) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 3 {
            return Err(MathMLError::InvalidStructure(
                "Subscript-superscript needs exactly 3 children".into(),
            ));
        }

        let base = self.process_element(&children[0])?;
        let subscript = self.process_element(&children[1])?;
        let superscript = self.process_element(&children[2])?;

        // Check if the base is a mathematical operator that should use multiline positioning
        let is_math_operator = if base.height == 1 && base.baseline == 0 {
            let base_text = base.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            matches!(base_text.as_str(), "∏" | "∑" | "∫" | "⋃" | "⋂" | "⋁" | "⋀")
        } else {
            false
        };

        // Try Unicode for simple cases first, but skip for mathematical operators
        if base.height == 1 && base.baseline == 0 && !is_math_operator {
            // Get subscript text
            let subscript_text = if subscript.height == 1 && subscript.baseline == 0 {
                subscript.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                // For multi-line subscripts, try to flatten them
                let flattened = subscript
                    .content
                    .iter()
                    .map(|row| row.iter().collect::<String>().trim().to_string())
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<String>>()
                    .join("")
                    .trim()
                    .to_string();

                if flattened.len() <= 10 {
                    flattened
                } else {
                    String::new()
                }
            };

            // Get superscript text
            let superscript_text = if superscript.height == 1 && superscript.baseline == 0 {
                superscript.content[0]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string()
            } else {
                String::new()
            };

            // Try Unicode if both are simple
            if !subscript_text.is_empty() && !superscript_text.is_empty() {
                if let (Some(unicode_sub), Some(unicode_sup)) = (
                    Self::try_unicode_subscript(&subscript_text, self.use_unicode),
                    Self::try_unicode_superscript(&superscript_text, self.use_unicode),
                ) {
                    let base_text = base.content[0]
                        .iter()
                        .collect::<String>()
                        .trim()
                        .to_string();
                    let combined_text = format!("{base_text}{unicode_sub}{unicode_sup}");
                    return Ok(MathBox::new(&combined_text));
                }
            }
        }

        // Fall back to multiline positioning
        if is_math_operator {
            // For summations, stack super/base/sub vertically and center
            let width = base.width.max(subscript.width).max(superscript.width);
            let height = superscript.height + base.height + subscript.height;
            let baseline = superscript.height + base.baseline;

            // Create result box
            let mut result = MathBox::create_empty(width, height, baseline);

            // Place superscript (centered above)
            let super_offset = (width.saturating_sub(superscript.width)) / 2;
            for y in 0..superscript.height {
                for x in 0..superscript.width {
                    result.set_char(x + super_offset, y, superscript.get_char(x, y));
                }
            }

            // Place base (centered in middle)
            let base_offset = (width.saturating_sub(base.width)) / 2;
            for y in 0..base.height {
                for x in 0..base.width {
                    result.set_char(x + base_offset, superscript.height + y, base.get_char(x, y));
                }
            }

            // Place subscript (centered below)
            let sub_offset = (width.saturating_sub(subscript.width)) / 2;
            for y in 0..subscript.height {
                for x in 0..subscript.width {
                    result.set_char(
                        x + sub_offset,
                        superscript.height + base.height + y,
                        subscript.get_char(x, y),
                    );
                }
            }

            Ok(result)
        } else {
            // For regular base with both sub and superscript, arrange diagonally
            let width = base.width + subscript.width.max(superscript.width);
            let height = superscript.height + base.height + subscript.height;
            let baseline = superscript.height + base.baseline;

            // Create result box
            let mut result = MathBox::create_empty(width, height, baseline);

            // Place base
            for y in 0..base.height {
                for x in 0..base.width {
                    result.set_char(x, superscript.height + y, base.get_char(x, y));
                }
            }

            // Place superscript (to the right and above)
            for y in 0..superscript.height {
                for x in 0..superscript.width {
                    result.set_char(base.width + x, y, superscript.get_char(x, y));
                }
            }

            // Place subscript (to the right and below)
            let sub_y_offset = superscript.height + base.height;
            for y in 0..subscript.height {
                for x in 0..subscript.width {
                    if sub_y_offset + y < height {
                        result.set_char(base.width + x, sub_y_offset + y, subscript.get_char(x, y));
                    }
                }
            }

            Ok(result)
        }
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
            .is_some_and(|text| text.contains('∑'));

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

    /// Process an underover element (like summation with both subscript and superscript)
    fn process_underover(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 3 {
            return Err(MathMLError::InvalidStructure(
                "Underover element needs exactly 3 children".into(),
            ));
        }

        let base = self.process_element(&children[0])?;
        let under = self.process_element(&children[1])?;
        let over = self.process_element(&children[2])?;

        // Check if this is a summation
        let is_summation = node
            .children()
            .find(|n| n.is_element())
            .and_then(|n| n.text())
            .is_some_and(|text| text.contains('∑'));

        // Calculate dimensions
        let mut width = base.width.max(under.width).max(over.width);
        if is_summation {
            width = width.max(2); // Ensure minimum width for summation
        }
        let height = over.height + base.height + under.height;
        let baseline = over.height + base.baseline;

        // Create result box
        let mut result = MathBox::create_empty(width, height, baseline);

        // Place over (centered above)
        let over_offset = (width.saturating_sub(over.width)) / 2;
        for y in 0..over.height {
            for x in 0..over.width {
                result.set_char(x + over_offset, y, over.get_char(x, y));
            }
        }

        // Place base (centered in middle)
        let base_offset = (width.saturating_sub(base.width)) / 2;
        for y in 0..base.height {
            for x in 0..base.width {
                result.set_char(x + base_offset, over.height + y, base.get_char(x, y));
            }
        }

        // Place under (centered below)
        let under_offset = (width.saturating_sub(under.width)) / 2;
        for y in 0..under.height {
            for x in 0..under.width {
                result.set_char(
                    x + under_offset,
                    over.height + base.height + y,
                    under.get_char(x, y),
                );
            }
        }

        Ok(result)
    }

    /// Generate square root radical symbol with given height and length
    fn generate_sqrt_radical(&self, height: usize, length: usize) -> Vec<String> {
        let height = if height < 3 { 3 } else { height }; // Minimum height

        let mut lines = Vec::new();

        // Top line: overline with diagonal start
        let top_padding = height + 1; // Space before the overline
        let overline = format!("⟋{}", "─".repeat(length));
        lines.push(format!("{}{}", " ".repeat(top_padding), overline));

        // Middle diagonal lines
        for i in 1..(height - 2) {
            let padding = height + 1 - i;
            lines.push(format!("{}╱  ", " ".repeat(padding)));
        }

        // Second to last line: connecting part
        if height > 2 {
            lines.push("_  ╱  ".to_string());
        }

        // Last line: tail
        lines.push(" \\╱  ".to_string());

        lines
    }

    /// Process a square root element
    fn process_square_root(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.is_empty() {
            return Ok(MathBox::new("√"));
        }

        // Step 1: Generate the inner formula content
        let inner = if children.len() == 1 {
            self.process_element(&children[0])?
        } else {
            // Multiple children - treat as horizontal group
            let boxes: Result<Vec<_>, _> = children
                .iter()
                .map(|child| self.process_element(child))
                .collect();
            let boxes = boxes?;
            self.horizontal_concat(boxes)
        };

        // For single line expressions, use simple format
        if inner.height == 1 {
            let inner_text = inner.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            return Ok(MathBox::new(&format!("√({inner_text})")));
        }

        // Step 2: Measure the formula dimensions
        let formula_width = inner.width;
        let formula_height = inner.height;

        // Step 3: Generate the radical symbol using our function
        // Add 1 to height to account for the overline space
        let radical_lines = self.generate_sqrt_radical(formula_height + 1, formula_width + 4);

        // Step 4: Calculate total dimensions
        let radical_width = radical_lines.first().map_or(0, |line| line.chars().count());
        let total_width = radical_width.max(formula_width + 10); // Extra padding
        let total_height = radical_lines.len();
        let baseline = inner.baseline + 1;

        // Create result box
        let mut result = MathBox::create_empty(total_width, total_height, baseline);

        // Place the radical symbol
        for (y, line) in radical_lines.iter().enumerate() {
            for (x, ch) in line.chars().enumerate() {
                if x < total_width && ch != ' ' {
                    result.set_char(x, y, ch);
                }
            }
        }

        // Step 5: Place the formula content in the space under the overline
        // Content should start after the diagonal space
        let content_x_offset = formula_height + 3; // Space for diagonal + padding
        let content_y_offset = 1; // Below the overline

        for y in 0..inner.height {
            for x in 0..inner.width {
                let ch = inner.get_char(x, y);
                if ch != ' ' && ch != '\0' {
                    let target_x = content_x_offset + x;
                    let target_y = content_y_offset + y;
                    if target_x < total_width && target_y < total_height {
                        result.set_char(target_x, target_y, ch);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process an nth root element (mroot)
    fn process_nth_root(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        if children.len() != 2 {
            return Err(MathMLError::InvalidStructure(
                "Nth root needs exactly 2 children (radicand and index)".into(),
            ));
        }

        // First child is the radicand (what's under the root)
        // Second child is the root index (the n in nth root)
        let radicand = self.process_element(&children[0])?;
        let index = self.process_element(&children[1])?;

        // For single line expressions with simple index, use Unicode format
        if radicand.height == 1 && index.height == 1 {
            let radicand_text = radicand.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            let index_text = index.content[0]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();

            // Try to convert index to superscript
            if let Some(unicode_index) =
                Self::try_unicode_superscript(&index_text, self.use_unicode)
            {
                // Use Unicode format: ³√x for cube root
                return Ok(MathBox::new(&format!("{unicode_index}√({radicand_text})")));
            } else {
                // Fallback to notation like: [3]√(x)
                return Ok(MathBox::new(&format!("[{index_text}]√({radicand_text})")));
            }
        }

        // For multi-line expressions, create ASCII art with proper alignment
        let formula_width = radicand.width;
        let formula_height = radicand.height;
        let index_text = index
            .content
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect::<Vec<String>>()
            .join("")
            .trim()
            .to_string();
        let index_width = index_text.len();

        let mut lines = Vec::new();

        // Use the EXACT SAME structure as square root!
        // For fraction height 3, we need 4 lines total with proper padding
        if formula_height == 3 {
            // Line 1: overline (padding = height + 1 = 5 spaces)
            let padding = " ".repeat(5);
            let dash_line = "─".repeat(formula_width + 4);
            lines.push(format!("{padding}⟋{dash_line}"));
            // Line 2: diagonal for numerator (padding = height + 1 - 1 = 4)
            lines.push(format!("{}╱  ", " ".repeat(4)));
            // Line 3: underscore + diagonal (this is where index goes)
            lines.push("_  ╱  ".to_string());
            // Line 4: bottom tail
            lines.push(" \\╱  ".to_string());
        } else {
            // For other heights, generate dynamically
            let radical_line_height = formula_height + 2;
            // Top line: overline with diagonal start
            let top_padding = radical_line_height - 1;
            let overline = format!("⟋{}", "─".repeat(formula_width + 4));
            lines.push(format!("{}{}", " ".repeat(top_padding), overline));

            // Generate diagonal lines
            for i in 1..radical_line_height {
                let padding = radical_line_height - 1 - i;
                if i == radical_line_height - 1 {
                    // Last line: tail at the bottom
                    lines.push(format!("{}\\╱  ", " ".repeat(padding)));
                } else if i == radical_line_height - 2 {
                    // Second to last line: connecting part with underscore
                    lines.push(format!("_{}╱  ", " ".repeat(padding)));
                } else {
                    // Middle diagonal lines
                    lines.push(format!("{}╱  ", " ".repeat(padding + 1)));
                }
            }
        }

        // Now prepend the index to the appropriate line
        // The index should go on the line with the underscore "_"
        let mut modified_lines = Vec::new();
        for line in lines.iter() {
            // Find the line that starts with underscore
            if line.trim_start().starts_with("_") {
                // This is the line with underscore - add index here WITH A SPACE
                // The underscore line has no leading spaces, so add one space before index
                modified_lines.push(format!(" {index_text}{line}"));
            } else {
                // Other lines need to be padded by index width + 1 for alignment
                modified_lines.push(format!("{}{}", " ".repeat(index_width + 1), line));
            }
        }

        // Calculate total dimensions
        let radical_width = modified_lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        let total_width = radical_width.max(formula_width + 10 + index_width);
        // Ensure total height is at least tall enough for the radicand content
        // The radicand starts at y_offset=1 (below the overline)
        let total_height = modified_lines.len().max(1 + radicand.height);
        let baseline = radicand.baseline + 1;

        // Create result box
        let mut result = MathBox::create_empty(total_width, total_height, baseline);

        // Place the radical symbol with index
        for (y, line) in modified_lines.iter().enumerate() {
            for (x, ch) in line.chars().enumerate() {
                if x < total_width && ch != ' ' {
                    result.set_char(x, y, ch);
                }
            }
        }

        // Place the formula content under the overline
        // Use SAME offset logic as square root!
        let content_x_offset = if formula_height == 3 {
            // sqrt uses formula_height + 3, we add index_width + 1 for the space
            7 + index_width // 4 + 3 for sqrt logic, adjusted for index placement
        } else {
            formula_height + 3 + index_width + 1 // Adjusted for index
        };
        let content_y_offset = 1; // Below the overline

        for y in 0..radicand.height {
            for x in 0..radicand.width {
                let ch = radicand.get_char(x, y);
                if ch != ' ' && ch != '\0' {
                    let target_x = content_x_offset + x;
                    let target_y = content_y_offset + y;
                    if target_x < total_width && target_y < total_height {
                        result.set_char(target_x, target_y, ch);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process a table element
    fn process_table(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let mut rows = Vec::new();
        let mut max_width = 0;

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "mtr" {
                let row_box = self.process_table_row(&child)?;
                max_width = max_width.max(row_box.width);
                rows.push(row_box);
            }
        }

        if rows.is_empty() {
            return Ok(MathBox::new(""));
        }

        // Check if this is a table with "where" clause (common in mathematical definitions)
        let mut has_where_clause = false;
        if rows.len() >= 2 {
            // Check if second row contains "where" text
            let mut row_idx = 0;
            for child in node.children().filter(|n| n.is_element()) {
                let tag = child.tag_name().name();
                if tag == "mtr" {
                    if row_idx == 1 {
                        // Check second row for "where" text
                        for td in child.children().filter(|n| n.is_element()) {
                            let td_tag = td.tag_name().name();
                            if td_tag == "mtd" {
                                for elem_child in td.children() {
                                    if let Some(text) = elem_child.text() {
                                        if text.to_lowercase().contains("where") {
                                            has_where_clause = true;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    row_idx += 1;
                }
            }
        }

        // Calculate total height, adding space if we have a where clause
        let mut total_height = rows.iter().map(|r| r.height).sum::<usize>();
        if has_where_clause && rows.len() >= 2 {
            total_height += 1; // Add empty line between main equation and where clause
        }

        // Create result box with proper dimensions
        let mut result = MathBox::create_empty(max_width, total_height, 0);

        // Place each row
        let mut y_offset = 0;
        for (i, row) in rows.iter().enumerate() {
            // Add empty line before "where" clause (second row)
            if has_where_clause && i == 1 {
                y_offset += 1;
            }

            // Center align if row is narrower than max width
            let x_offset = (max_width.saturating_sub(row.width)) / 2;
            for y in 0..row.height {
                for x in 0..row.width {
                    let ch = row.get_char(x, y);
                    // Copy all characters, including spaces, to preserve row structure
                    result.set_char(x + x_offset, y + y_offset, ch);
                }
            }
            y_offset += row.height;
        }

        // Set baseline to middle of table
        result.baseline = total_height / 2;

        Ok(result)
    }

    /// Process a table row element
    fn process_table_row(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let mut cells = Vec::new();

        for child in node.children().filter(|n| n.is_element()) {
            let tag = child.tag_name().name();
            if tag == "mtd" {
                let cell_box = self.process_table_cell(&child)?;
                cells.push(cell_box);
            }
        }

        if cells.is_empty() {
            return Ok(MathBox::new(""));
        }

        // Concatenate cells horizontally with spacing
        let mut boxes_with_spacing = Vec::new();
        for (i, cell) in cells.iter().enumerate() {
            boxes_with_spacing.push(cell.clone());
            if i < cells.len() - 1 {
                // Add spacing between cells
                boxes_with_spacing.push(MathBox::new("  "));
            }
        }

        Ok(self.horizontal_concat(boxes_with_spacing))
    }

    /// Process a table cell element
    fn process_table_cell(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();

        if !children.is_empty() {
            // Process cell content
            let mut boxes = Vec::new();
            for child in &children {
                let box_item = self.process_element(child)?;
                if box_item.width > 0 {
                    boxes.push(box_item);
                }
            }

            if !boxes.is_empty() {
                return Ok(self.horizontal_concat(boxes));
            }
        }

        // Handle text content
        if let Some(text) = node.text() {
            return Ok(MathBox::new(text));
        }

        Ok(MathBox::new(""))
    }

    /// Process a fenced expression (with braces, brackets, etc.)
    fn process_fenced(&self, node: &roxmltree::Node) -> Result<MathBox, MathMLError> {
        // Get opening and closing delimiters
        let open_delim = node.attribute("open").unwrap_or("(");
        let close_delim = node.attribute("close").unwrap_or(")");
        let separators = node.attribute("separators").unwrap_or(",");

        // Process the content
        let children: Vec<_> = node.children().filter(|n| n.is_element()).collect();
        let content_box = if !children.is_empty() {
            // Process ALL children, not just the first one
            let mut boxes = Vec::new();
            for (i, child) in children.iter().enumerate() {
                let child_box = self.process_element(child)?;
                if child_box.width > 0 {
                    boxes.push(child_box);
                    // Add separator between elements if specified
                    if i < children.len() - 1 && !separators.is_empty() {
                        // Only add separator if it's not empty
                        boxes.push(MathBox::new(separators));
                    }
                }
            }

            if !boxes.is_empty() {
                self.horizontal_concat(boxes)
            } else {
                MathBox::new("")
            }
        } else {
            MathBox::new("")
        };

        // Special handling for braces - need multi-line
        if open_delim == "{" && content_box.height > 1 {
            return Ok(self.create_multi_line_brace(&content_box));
        }

        // For single-line content, just add delimiters
        if content_box.height == 1 {
            let result_text = format!(
                "{}{}{}",
                open_delim,
                content_box.content[0].iter().collect::<String>(),
                close_delim
            );
            return Ok(MathBox::new(&result_text));
        }

        // For multi-line content with parentheses/brackets
        Ok(self.create_multi_line_delimiters(&content_box, open_delim, close_delim))
    }

    /// Create a multi-line brace around content
    fn create_multi_line_brace(&self, content: &MathBox) -> MathBox {
        let height = content.height;
        let width = content.width + 2; // Space for brace

        let mut result = MathBox::create_empty(width, height, content.baseline);

        // Draw left brace
        if height == 1 {
            result.set_char(0, 0, '{');
        } else if height == 2 {
            result.set_char(0, 0, '⎧');
            result.set_char(0, 1, '⎩');
        } else if height == 3 {
            result.set_char(0, 0, '⎧');
            result.set_char(0, 1, '⎨');
            result.set_char(0, 2, '⎩');
        } else {
            // For larger braces
            result.set_char(0, 0, '⎧');
            for i in 1..height - 1 {
                if i == height / 2 {
                    result.set_char(0, i, '⎨');
                } else {
                    result.set_char(0, i, '⎪');
                }
            }
            result.set_char(0, height - 1, '⎩');
        }

        // Copy content
        for y in 0..content.height {
            for x in 0..content.width {
                let ch = content.get_char(x, y);
                if ch != ' ' && ch != '\0' {
                    result.set_char(x + 1, y, ch);
                }
            }
        }

        result
    }

    /// Create multi-line delimiters (parentheses, brackets) around content
    fn create_multi_line_delimiters(
        &self,
        content: &MathBox,
        open_delim: &str,
        close_delim: &str,
    ) -> MathBox {
        let height = content.height;
        let width = content.width + 2; // Space for delimiters

        let mut result = MathBox::create_empty(width, height, content.baseline);

        // Map delimiters to multi-line versions
        if open_delim == "(" {
            if height == 2 {
                result.set_char(0, 0, '⎛');
                result.set_char(0, 1, '⎝');
            } else {
                result.set_char(0, 0, '⎛');
                for i in 1..height - 1 {
                    result.set_char(0, i, '⎜');
                }
                result.set_char(0, height - 1, '⎝');
            }
        } else if open_delim == "[" {
            if height == 2 {
                result.set_char(0, 0, '⎡');
                result.set_char(0, 1, '⎣');
            } else {
                result.set_char(0, 0, '⎡');
                for i in 1..height - 1 {
                    result.set_char(0, i, '⎢');
                }
                result.set_char(0, height - 1, '⎣');
            }
        }

        // Copy content
        for y in 0..content.height {
            for x in 0..content.width {
                let ch = content.get_char(x, y);
                if ch != ' ' {
                    result.set_char(x + 1, y, ch);
                }
            }
        }

        // Add closing delimiter
        if close_delim == ")" {
            if height == 2 {
                result.set_char(width - 1, 0, '⎞');
                result.set_char(width - 1, 1, '⎠');
            } else {
                result.set_char(width - 1, 0, '⎞');
                for i in 1..height - 1 {
                    result.set_char(width - 1, i, '⎟');
                }
                result.set_char(width - 1, height - 1, '⎠');
            }
        } else if close_delim == "]" {
            if height == 2 {
                result.set_char(width - 1, 0, '⎤');
                result.set_char(width - 1, 1, '⎦');
            } else {
                result.set_char(width - 1, 0, '⎤');
                for i in 1..height - 1 {
                    result.set_char(width - 1, i, '⎥');
                }
                result.set_char(width - 1, height - 1, '⎦');
            }
        }

        result
    }

    /// Check if a MathBox contains mathematical operators that need spacing
    fn contains_math_operator_needing_spacing(&self, math_box: &MathBox) -> bool {
        if math_box.height == 0 {
            return false;
        }

        // Only add spacing for complex multi-line mathematical operators that have
        // both subscript and superscript (like ∏ with both i=1 below and n above)
        // Don't add spacing for simple munder elements (like ∑ with just subscript)
        if math_box.height > 2 {
            // Need at least 3 lines for subscript + operator + superscript
            for row in &math_box.content {
                let text: String = row.iter().collect();
                if text
                    .chars()
                    .any(|c| matches!(c, '∏' | '∑' | '∫' | '⋃' | '⋂' | '⋁' | '⋀'))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Add spacing around mathematical operators in a list of boxes
    fn add_operator_spacing(&self, boxes: Vec<MathBox>) -> Vec<MathBox> {
        let mut result = Vec::new();

        for (i, box_item) in boxes.iter().enumerate() {
            // Add space before mathematical operators (except at the beginning)
            if i > 0 && self.contains_math_operator_needing_spacing(box_item) {
                result.push(MathBox::new(" "));
            }

            result.push(box_item.clone());

            // Add space after mathematical operators (except at the end)
            if i < boxes.len() - 1 && self.contains_math_operator_needing_spacing(box_item) {
                result.push(MathBox::new(" "));
            }
        }

        result
    }

    /// Check if a box contains only a single parenthesis character
    fn is_single_paren(&self, math_box: &MathBox) -> Option<char> {
        if math_box.height == 1 && math_box.width == 1 {
            let text: String = math_box.content[0].iter().collect();
            let ch = text.chars().next()?;
            if matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
                Some(ch)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Create multi-line parenthesis for given height
    fn create_multiline_paren(&self, paren_char: char, height: usize) -> MathBox {
        if height < 3 {
            // For small expressions, use regular parentheses
            return MathBox::new(&paren_char.to_string());
        }

        let mut content = Vec::new();

        match paren_char {
            '(' => {
                // First line: ⎛
                content.push(vec!['⎛']);
                // Middle lines: ⎜
                for _ in 1..height - 1 {
                    content.push(vec!['⎜']);
                }
                // Last line: ⎝
                content.push(vec!['⎝']);
            }
            ')' => {
                // First line: ⎞
                content.push(vec!['⎞']);
                // Middle lines: ⎟
                for _ in 1..height - 1 {
                    content.push(vec!['⎟']);
                }
                // Last line: ⎠
                content.push(vec!['⎠']);
            }
            '[' => {
                // For square brackets, use similar characters or fall back to regular
                // First line: ⎡
                content.push(vec!['⎡']);
                // Middle lines: ⎢
                for _ in 1..height - 1 {
                    content.push(vec!['⎢']);
                }
                // Last line: ⎣
                content.push(vec!['⎣']);
            }
            ']' => {
                // First line: ⎤
                content.push(vec!['⎤']);
                // Middle lines: ⎥
                for _ in 1..height - 1 {
                    content.push(vec!['⎥']);
                }
                // Last line: ⎦
                content.push(vec!['⎦']);
            }
            '{' => {
                // For curly braces, use similar approach
                // First line: ⎧
                content.push(vec!['⎧']);
                // Middle lines: ⎨ (only one in the middle) or ⎪ for multiple
                if height == 3 {
                    content.push(vec!['⎨']);
                } else {
                    for i in 1..height - 1 {
                        if i == height / 2 {
                            content.push(vec!['⎨']); // Middle connector
                        } else {
                            content.push(vec!['⎪']); // Vertical line
                        }
                    }
                }
                // Last line: ⎩
                content.push(vec!['⎩']);
            }
            '}' => {
                // First line: ⎫
                content.push(vec!['⎫']);
                // Middle lines: ⎬ (only one in the middle) or ⎪ for multiple
                if height == 3 {
                    content.push(vec!['⎬']);
                } else {
                    for i in 1..height - 1 {
                        if i == height / 2 {
                            content.push(vec!['⎬']); // Middle connector
                        } else {
                            content.push(vec!['⎪']); // Vertical line
                        }
                    }
                }
                // Last line: ⎭
                content.push(vec!['⎭']);
            }
            _ => {
                // Fallback: use regular character
                for _ in 0..height {
                    content.push(vec![paren_char]);
                }
            }
        }

        MathBox {
            width: 1,
            height,
            baseline: height / 2, // Middle baseline
            content,
        }
    }

    /// Replace single parentheses with multi-line versions when appropriate
    fn upgrade_parentheses(&self, boxes: Vec<MathBox>) -> Vec<MathBox> {
        if boxes.len() < 3 {
            return boxes; // Need at least opening paren, content, closing paren
        }

        let mut result = Vec::new();
        let mut i = 0;

        while i < boxes.len() {
            // Check for pattern: opening_paren + content + closing_paren
            if i + 2 < boxes.len() {
                if let Some(open_char) = self.is_single_paren(&boxes[i]) {
                    // Look ahead to find matching closing parenthesis
                    let mut paren_count = 1;
                    let mut closing_pos = None;

                    for (j, candidate) in boxes.iter().enumerate().skip(i + 1) {
                        if let Some(ch) = self.is_single_paren(candidate) {
                            match (open_char, ch) {
                                ('(', ')') | ('[', ']') | ('{', '}') => {
                                    paren_count -= 1;
                                    if paren_count == 0 {
                                        closing_pos = Some(j);
                                        break;
                                    }
                                }
                                ('(', '(') | ('[', '[') | ('{', '{') => {
                                    paren_count += 1;
                                }
                                _ => {}
                            }
                        }
                    }

                    // If we found a matching closing parenthesis
                    if let Some(close_idx) = closing_pos {
                        // Check the height of content between parentheses
                        let content_boxes = &boxes[i + 1..close_idx];
                        let max_height = content_boxes.iter().map(|b| b.height).max().unwrap_or(0);

                        if max_height >= 3 {
                            // Use multi-line parentheses
                            let close_char = match open_char {
                                '(' => ')',
                                '[' => ']',
                                '{' => '}',
                                _ => open_char,
                            };

                            result.push(self.create_multiline_paren(open_char, max_height));
                            result.extend_from_slice(content_boxes);
                            result.push(self.create_multiline_paren(close_char, max_height));

                            i = close_idx + 1;
                            continue;
                        }
                    }
                }
            }

            // Default case: just add the box as-is
            result.push(boxes[i].clone());
            i += 1;
        }

        result
    }

    /// Concatenate boxes horizontally, aligning at baseline
    fn horizontal_concat(&self, boxes: Vec<MathBox>) -> MathBox {
        // Filter out empty boxes
        let mut boxes: Vec<_> = boxes
            .into_iter()
            .filter(|b| b.width > 0 && b.height > 0)
            .collect();

        if boxes.is_empty() {
            return MathBox::new("");
        }

        if boxes.len() == 1 {
            return boxes.into_iter().next().unwrap();
        }

        // Add spacing around mathematical operators
        boxes = self.add_operator_spacing(boxes);

        // Upgrade single parentheses to multi-line when appropriate
        boxes = self.upgrade_parentheses(boxes);

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
                                .first()
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

    let math_pattern = Regex::new(r"(?s)<math[^>]*>.*?</math>").unwrap();
    let matches: Vec<_> = math_pattern.find_iter(html).collect();

    if matches.is_empty() {
        return Ok(html.to_string());
    }

    let mathml = matches[0].as_str();
    let math_box = parser.parse(mathml)?;
    Ok(math_box.render())
}

#[cfg(test)]
mod tests {
    use super::*;
    //
    //  DO NOT REMOVE THIS!
    //
    //        let expected = r#"
    //      ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲ ̲
    //    ╱       x² + b_c
    //_  ╱  ───────────────────────
    // \╱     sin(x)ᶜᵒˢ⁽ʸ⁾ + eᶻ·⁵    "#;

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
                            P(x)        ⎡ P(x)⎤
E    [x] = ∑ P(x)x = ∑ Q(x)x──── = E    ⎢x────⎥
 P(x)      x         x      Q(x)    Q(x)⎣ Q(x)⎦"#
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

    #[test]
    fn test_likelihood_function() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper L left-parenthesis x semicolon theta right-parenthesis equals sigma-summation Underscript j Endscripts upper C Subscript i j Baseline upper P left-parenthesis j vertical-bar x semicolon theta right-parenthesis">
  <mrow>
    <mi>L</mi>
    <mrow>
      <mo>(</mo>
      <mi>x</mi>
      <mo>;</mo>
      <mi>θ</mi>
      <mo>)</mo>
    </mrow>
    <mo>=</mo>
    <msub><mo>∑</mo> <mi>j</mi> </msub>
    <msub><mi>C</mi> <mrow><mi>i</mi><mi>j</mi></mrow> </msub>
    <mi>P</mi>
    <mrow>
      <mo>(</mo>
      <mi>j</mi>
      <mo>|</mo>
      <mi>x</mi>
      <mo>;</mo>
      <mi>θ</mi>
      <mo>)</mo>
    </mrow>
  </mrow>
</math>
        "#;

        let expected = "L(x;θ) = ∑ⱼCᵢⱼP(j|x;θ)";
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_average_likelihood_with_fraction() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper L left-parenthesis upper X semicolon theta right-parenthesis equals sigma-summation Underscript x Endscripts StartFraction 1 Over upper N EndFraction upper L left-parenthesis x semicolon theta right-parenthesis">
  <mrow>
    <mi>L</mi>
    <mrow>
      <mo>(</mo>
      <mi>X</mi>
      <mo>;</mo>
      <mi>θ</mi>
      <mo>)</mo>
    </mrow>
    <mo>=</mo>
    <msub><mo>∑</mo> <mi>x</mi> </msub>
    <mfrac><mn>1</mn> <mi>N</mi></mfrac>
    <mi>L</mi>
    <mrow>
      <mo>(</mo>
      <mi>x</mi>
      <mo>;</mo>
      <mi>θ</mi>
      <mo>)</mo>
    </mrow>
  </mrow>
</math>
        "#;

        let expected = r#"1
L(X;θ) = ∑ₓ─L(x;θ)
           N"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_weight_formula_with_text() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper W Subscript i Baseline equals StartFraction upper N Over number of samples of class reverse-solidus emph left-brace i right-brace EndFraction">
  <mrow>
    <msub><mi>W</mi> <mi>i</mi> </msub>
    <mo>=</mo>
    <mfrac><mi>N</mi> <mrow><mtext>number</mtext><mspace width="4.pt"/><mtext>of</mtext><mspace width="4.pt"/><mtext>samples</mtext><mspace width="4.pt"/><mtext>of</mtext><mspace width="4.pt"/><mtext>class</mtext><mspace width="4.pt"/><mtext>i</mtext></mrow></mfrac>
  </mrow>
</math>
        "#;

        let expected = r#"N
Wᵢ = ────────────────────────────
     number of samples of class i"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_mspace_handling() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mrow>
                <mtext>hello</mtext>
                <mspace width="5pt"/>
                <mtext>world</mtext>
            </mrow>
        </math>
        "#;

        let expected = "hello world";
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_nested_subscript_superscript() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="sigma-summation Underscript j Endscripts e Superscript x Super Subscript j">
  <mrow>
    <msub><mo>∑</mo> <mi>j</mi> </msub>
    <msup><mi>e</mi> <msub><mi>x</mi> <mi>j</mi> </msub> </msup>
  </mrow>
</math>
        "#;

        let expected = r#"xⱼ
∑ⱼe"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_latex_notation_subscripts() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msub>
                <mi>A</mi>
                <mi>bc</mi>
            </msub>
        </math>
        "#;

        let expected = "A_bc";
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_latex_notation_superscripts() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msup>
                <mi>y</mi>
                <mi>q1</mi>
            </msup>
        </math>
        "#;

        let expected = "y^q1";
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);
    }

    #[test]
    fn test_latex_notation_fallback_to_multiline() {
        let mathml = r#"
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msub>
                <mi>x</mi>
                <mi>abc</mi>
            </msub>
        </math>
        "#;

        // Now uses LaTeX notation since "abc" is 3 characters (≤ 5 and alphanumeric)
        let expected = r#"x_abc"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_attention_mechanism() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="Attention left-parenthesis upper Q comma upper K comma upper V right-parenthesis equals softmax left-parenthesis StartFraction upper Q upper K Superscript upper T Baseline Over StartRoot d EndRoot EndFraction right-parenthesis upper V">
  <mrow>
    <mtext>Attention</mtext>
    <mrow>
      <mo>(</mo>
      <mi>Q</mi>
      <mo>,</mo>
      <mi>K</mi>
      <mo>,</mo>
      <mi>V</mi>
      <mo>)</mo>
    </mrow>
    <mo>=</mo>
    <mi> softmax </mi>
    <mrow>
      <mo>(</mo>
      <mfrac><mrow><mi>Q</mi><msup><mi>K</mi> <mi>T</mi> </msup></mrow> <msqrt><mi>d</mi></msqrt></mfrac>
      <mo>)</mo>
    </mrow>
    <mi>V</mi>
  </mrow>
</math>
        "#;

        let expected = r#"
                            ⎛QKᵀ ⎞
Attention(Q,K,V) =  softmax ⎜────⎟V
                            ⎝√(d)⎠"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_complex_square_root() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
  <msqrt>
    <mfrac>
      <mrow>
        <msup><mi>x</mi> <mn>2</mn> </msup>
        <mo>+</mo>
        <msub><mi>b</mi> <mi>c</mi> </msub>
      </mrow>
      <mrow>
        <msup>
          <mrow>
            <mo form="prefix">sin</mo>
            <mrow>
              <mo>(</mo>
              <mi>x</mi>
              <mo>)</mo>
            </mrow>
          </mrow>
          <mrow>
            <mo form="prefix">cos</mo>
            <mrow>
              <mo>(</mo>
              <mi>y</mi>
              <mo>)</mo>
            </mrow>
          </mrow>
        </msup>
        <mo>+</mo>
        <msup><mi>e</mi>
          <mrow>
            <mi>z</mi>
            <mo>*</mo>
            <mn>5</mn>
          </mrow>
        </msup>
      </mrow>
    </mfrac>
  </msqrt>
</math>
        "#;

        let expected = r#"
      ⟋─────────────────────────
     ╱       x² + b_c
    ╱  ─────────────────────
_  ╱                   z * 5
 \╱    sin(x)ᶜᵒˢ⁽ʸ⁾ + e"#;

        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    #[test]
    fn test_probability_formula_with_product() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="upper P left-parenthesis x 1 comma x 2 comma period period period comma x Subscript n Baseline right-parenthesis Superscript minus StartFraction 1 Over n EndFraction Baseline equals left-parenthesis StartFraction 1 Over upper P left-parenthesis x 1 comma x 2 comma ellipsis comma x Subscript n Baseline right-parenthesis EndFraction right-parenthesis Superscript StartFraction 1 Over n EndFraction Baseline equals left-parenthesis product Underscript i equals 1 Overscript n Endscripts StartFraction 1 Over upper P left-parenthesis x Subscript i Baseline vertical-bar x 1 comma period period period comma x Subscript i minus 1 Baseline right-parenthesis EndFraction right-parenthesis Superscript StartFraction 1 Over n EndFraction">
  <mrow>
    <mi>P</mi>
    <msup><mrow><mo>(</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><msub><mi>x</mi> <mn>2</mn> </msub><mo>,</mo><mo>.</mo><mo>.</mo><mo>.</mo><mo>,</mo><msub><mi>x</mi> <mi>n</mi> </msub><mo>)</mo></mrow> <mrow><mo>-</mo><mfrac><mn>1</mn> <mi>n</mi></mfrac></mrow> </msup>
    <mo>=</mo>
    <msup><mrow><mo>(</mo><mfrac><mn>1</mn> <mrow><mi>P</mi><mo>(</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><msub><mi>x</mi> <mn>2</mn> </msub><mo>,</mo><mi>â</mi><mi></mi><mi>¦</mi><mo>,</mo><msub><mi>x</mi> <mi>n</mi> </msub><mo>)</mo></mrow></mfrac><mo>)</mo></mrow> <mfrac><mn>1</mn> <mi>n</mi></mfrac> </msup>
    <mo>=</mo>
    <msup><mrow><mo>(</mo><msubsup><mo>∏</mo> <mrow><mi>i</mi><mo>=</mo><mn>1</mn></mrow> <mi>n</mi> </msubsup><mfrac><mn>1</mn> <mrow><mi>P</mi><mo>(</mo><msub><mi>x</mi> <mi>i</mi> </msub><mo>|</mo><msub><mi>x</mi> <mn>1</mn> </msub><mo>,</mo><mo>.</mo><mo>.</mo><mo>.</mo><mo>,</mo><msub><mi>x</mi> <mrow><mi>i</mi><mo>-</mo><mn>1</mn></mrow> </msub><mo>)</mo></mrow></mfrac><mo>)</mo></mrow> <mfrac><mn>1</mn> <mi>n</mi></mfrac> </msup>
  </mrow>
</math>
        "#;

        let expected = r#"
                                      1                                1
                  1                   ─                                ─
                - ─                   n                                n
                  n   ⎛      1       ⎞     ⎛   n            1         ⎞
P(x₁,x₂,...,xₙ)     = ⎜──────────────⎟  =  ⎜   ∏   ───────────────────⎟
                      ⎝P(x₁,x₂,â¦,xₙ)⎠     ⎝ i = 1 P(xᵢ|x₁,...,xᵢ ₋ ₁)⎠"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected.trim());
    }

    fn assert_multiline_eq(actual: &str, expected: &str) {
        let actual_lines: Vec<&str> = actual.lines().collect();
        let expected_lines: Vec<&str> = expected.lines().collect();

        if actual != expected {
            println!("\n=== ASSERTION FAILED ===");
            println!(
                "Expected {} lines, got {} lines\n",
                expected_lines.len(),
                actual_lines.len()
            );

            let max_lines = actual_lines.len().max(expected_lines.len());
            for i in 0..max_lines {
                let actual_line = actual_lines.get(i).unwrap_or(&"<missing>");
                let expected_line = expected_lines.get(i).unwrap_or(&"<missing>");

                if actual_line != expected_line {
                    println!("Line {} differs:", i + 1);
                    println!("  Expected: {expected_line:?}");
                    println!("  Actual:   {actual_line:?}");
                } else {
                    println!("Line {} matches: {:?}", i + 1, actual_line);
                }
            }

            println!("\n=== FULL EXPECTED OUTPUT ===");
            println!("{expected}");
            println!("\n=== FULL ACTUAL OUTPUT ===");
            println!("{actual}");
            println!("=== END ===\n");

            panic!("Multi-line strings don't match");
        }
    }

    #[test]
    fn test_complex_table_with_fenced_expression() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">
  <mtable displaystyle="true">
    <mtr>
      <mtd columnalign="right">
        <mrow>
          <mi>J</mi>
          <mo>(</mo>
          <mi>k</mi>
          <mo>,</mo>
          <msub><mi>t</mi> <mi>k</mi> </msub>
          <mo>)</mo>
        </mrow>
      </mtd>
      <mtd columnalign="left">
        <mrow>
          <mo>=</mo>
          <mfrac><msub><mi>m</mi> <mtext>left</mtext> </msub> <mi>m</mi></mfrac>
          <mspace width="0.166667em"></mspace>
          <msub><mtext>G</mtext> <mtext>left</mtext> </msub>
          <mo>+</mo>
          <mfrac><msub><mi>m</mi> <mtext>right</mtext> </msub> <mi>m</mi></mfrac>
          <mspace width="0.166667em"></mspace>
          <msub><mtext>G</mtext> <mtext>right</mtext> </msub>
        </mrow>
      </mtd>
    </mtr>
    <mtr>
      <mtd columnalign="right">
        <mrow>
          <mtext>where</mtext>
          <mspace width="1.em"></mspace>
        </mrow>
      </mtd>
      <mtd columnalign="left">
        <mfenced separators="" open="{" close="">
          <mtable>
            <mtr>
              <mtd columnalign="left">
                <mrow>
                  <msub><mtext>G</mtext> <mtext>left/right</mtext> </msub>
                  <mspace width="4.pt"></mspace>
                  <mtext>measures</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>the</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>impurity</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>of</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>the</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>left/right</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>subset</mtext>
                </mrow>
              </mtd>
            </mtr>
            <mtr>
              <mtd columnalign="left">
                <mrow>
                  <msub><mi>m</mi> <mtext>left/right</mtext> </msub>
                  <mspace width="4.pt"></mspace>
                  <mtext>is</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>the</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>number</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>of</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>instances</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>in</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>the</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>left/right</mtext>
                  <mspace width="4.pt"></mspace>
                  <mtext>subset</mtext>
                </mrow>
              </mtd>
            </mtr>
            <mtr>
              <mtd columnalign="left">
                <mrow>
                  <mi>m</mi>
                  <mo>=</mo>
                  <msub><mi>m</mi> <mtext>left</mtext> </msub>
                  <mo>+</mo>
                  <msub><mi>m</mi> <mtext>right</mtext> </msub>
                </mrow>
              </mtd>
            </mtr>
          </mtable>
        </mfenced>
      </mtd>
    </mtr>
  </mtable>
</math>
        "#;

        let expected = r#"                           m_left          m_right
               J(k,tₖ)   = ────── G_left + ─────── G_right
                             m                m
        ⎧  G_left/right measures the impurity of the left/right subset
where   ⎨m_left/right is the number of instances in the left/right subset
        ⎩                      m = m_left + m_right"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_multiline_eq(result.trim(), expected.trim());
    }

    #[test]
    fn test_entropy_formula_with_munderover() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" display="block">
  <mrow>
    <msub><mi>H</mi> <mi>i</mi> </msub>
    <mo>=</mo>
    <mo>-</mo>
    <munderover><mo>∑</mo> <mfrac linethickness="0pt"><mrow><mi>k</mi><mo>=</mo><mn>1</mn></mrow> <mrow><msub><mi>p</mi> <mrow><mi>i</mi><mo>,</mo><mi>k</mi></mrow> </msub><mo>≠</mo><mn>0</mn></mrow></mfrac> <mi>n</mi> </munderover>
    <mrow>
      <msub><mi>p</mi> <mrow><mi>i</mi><mo>,</mo><mi>k</mi></mrow> </msub>
      <msub><mo form="prefix">log</mo> <mn>2</mn> </msub>
      <mrow>
        <mo>(</mo>
        <msub><mi>p</mi> <mrow><mi>i</mi><mo>,</mo><mi>k</mi></mrow> </msub>
        <mo>)</mo>
      </mrow>
    </mrow>
  </mrow>
</math>
        "#;

        let expected = r#"            n
Hᵢ =  -     ∑     pᵢ,ₖlog₂(pᵢ,ₖ)
          k = 1
         pᵢ,ₖ ≠ 0"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_multiline_eq(result.trim(), expected.trim());
    }

    #[test]
    fn test_rmse_formula() {
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="RMSE left-parenthesis bold upper X comma bold y comma h right-parenthesis equals StartRoot StartFraction 1 Over m EndFraction sigma-summation Underscript i equals 1 Overscript m Endscripts left-parenthesis h left-parenthesis bold x Superscript left-parenthesis i right-parenthesis Baseline right-parenthesis minus y Superscript left-parenthesis i right-parenthesis Baseline right-parenthesis squared EndRoot" display="block">
  <mrow>
    <mtext>RMSE</mtext>
    <mfenced separators="" open="(" close=")">
      <mi>𝐗</mi>
      <mo>,</mo>
      <mi>𝐲</mi>
      <mo>,</mo>
      <mi>h</mi>
    </mfenced>
    <mo>=</mo>
    <msqrt>
      <mrow>
        <mfrac><mn>1</mn> <mi>m</mi></mfrac>
        <msubsup><mo>∑</mo> <mrow><mi>i</mi><mo>=</mo><mn>1</mn></mrow> <mi>m</mi> </msubsup>
        <msup><mrow><mfenced separators="" open="(" close=")"><mi>h</mi><mfenced separators="" open="(" close=")"><msup><mrow><mi>𝐱</mi></mrow> <mfenced open="(" close=")"><mi>i</mi></mfenced> </msup></mfenced><mo>-</mo><msup><mrow><mi>y</mi></mrow> <mfenced open="(" close=")"><mi>i</mi></mfenced> </msup></mfenced></mrow> <mn>2</mn> </msup>
      </mrow>
    </msqrt>
  </mrow>
</math>
        "#;

        let expected = r#"                   ⟋───────────────────────────────
                   ╱ 1     m
RMSE(𝐗,𝐲,h) =  _  ╱  ─     ∑   (h(𝐱⁽ⁱ⁾) - y⁽ⁱ⁾)²
                \╱   m   i = 1"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_multiline_eq(result.trim(), expected.trim());
    }

    #[test]
    fn test_mroot_single_line() {
        // Test single-line nth root cases
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mroot>
        <mn>8</mn>
        <mn>3</mn>
    </mroot>
</math>
        "#;

        let expected = "³√(8)";
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_eq!(result.trim(), expected);

        // Test with expression
        let mathml2 = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mroot>
        <mrow>
            <msup>
                <mi>x</mi>
                <mn>2</mn>
            </msup>
            <mo>+</mo>
            <mn>1</mn>
        </mrow>
        <mn>4</mn>
    </mroot>
</math>
        "#;

        let expected2 = "⁴√(x² + 1)";
        let result2 = mathml_to_ascii(mathml2, true).unwrap();
        assert_eq!(result2.trim(), expected2);

        // Test with variable index
        let mathml3 = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mroot>
        <mi>a</mi>
        <mi>n</mi>
    </mroot>
</math>
        "#;

        let expected3 = "ⁿ√(a)";
        let result3 = mathml_to_ascii(mathml3, true).unwrap();
        assert_eq!(result3.trim(), expected3);
    }

    #[test]
    fn test_mroot_multiline() {
        // Test multi-line nth root with fraction
        let mathml = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mroot>
        <mfrac>
            <mrow>
                <mi>x</mi>
                <mo>+</mo>
                <mi>y</mi>
            </mrow>
            <mrow>
                <mi>z</mi>
                <mo>-</mo>
                <mn>1</mn>
            </mrow>
        </mfrac>
        <mn>5</mn>
    </mroot>
</math>
        "#;

        let expected = r#"
       ⟋─────────
      ╱ x + y
 5_  ╱  ─────
   \╱   z - 1"#;
        let result = mathml_to_ascii(mathml, true).unwrap();
        assert_multiline_eq(result.trim_start(), expected.trim_start());
    }
}
