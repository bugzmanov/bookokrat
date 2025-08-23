#!/usr/bin/env python3
"""
MathML to ASCII converter for terminal rendering.
Parses MathML expressions and generates properly positioned ASCII art.
"""

import re
from dataclasses import dataclass
from typing import List, Optional, Tuple
from xml.etree import ElementTree as ET


@dataclass
class MathBox:
    """Represents a rendered math element with its dimensions and content."""
    width: int
    height: int
    baseline: int  # Distance from top to baseline
    content: List[List[str]]  # 2D grid of characters
    
    def __init__(self, text: str = ""):
        """Initialize a simple text box."""
        self.width = len(text)
        self.height = 1
        self.baseline = 0
        self.content = [[c for c in text]] if text else [[]]
    
    def get_char(self, x: int, y: int) -> str:
        """Get character at position, return space if out of bounds."""
        if 0 <= y < self.height and 0 <= x < self.width:
            return self.content[y][x]
        return ' '
    
    def set_char(self, x: int, y: int, char: str):
        """Set character at position."""
        if 0 <= y < self.height and 0 <= x < self.width:
            self.content[y][x] = char
    
    @staticmethod
    def create_empty(width: int, height: int, baseline: int) -> 'MathBox':
        """Create an empty box with given dimensions."""
        box = MathBox()
        box.width = width
        box.height = height
        box.baseline = baseline
        box.content = [[' ' for _ in range(width)] for _ in range(height)]
        return box
    
    def render(self) -> str:
        """Render the box as a string."""
        return '\n'.join(''.join(row) for row in self.content)


class MathMLParser:
    """Parser for MathML expressions."""
    
    def __init__(self, use_unicode=True):
        self.namespace = {'m': 'http://www.w3.org/1998/Math/MathML'}
        self.use_unicode = use_unicode
        
        # Unicode subscript mappings
        self.unicode_subscripts = {
            '0': '₀', '1': '₁', '2': '₂', '3': '₃', '4': '₄', '5': '₅', '6': '₆', '7': '₇', '8': '₈', '9': '₉',
            'a': 'ₐ', 'e': 'ₑ', 'i': 'ᵢ', 'o': 'ₒ', 'u': 'ᵤ', 'x': 'ₓ', 'h': 'ₕ', 'k': 'ₖ', 'l': 'ₗ', 'm': 'ₘ', 
            'n': 'ₙ', 'p': 'ₚ', 'r': 'ᵣ', 's': 'ₛ', 't': 'ₜ', 'v': 'ᵥ', 'ə': 'ₔ',
            '+': '₊', '-': '₋', '=': '₌', '(': '₍', ')': '₎'
        }
        
        # Unicode superscript mappings
        self.unicode_superscripts = {
            '0': '⁰', '1': '¹', '2': '²', '3': '³', '4': '⁴', '5': '⁵', '6': '⁶', '7': '⁷', '8': '⁸', '9': '⁹',
            'a': 'ᵃ', 'b': 'ᵇ', 'c': 'ᶜ', 'd': 'ᵈ', 'e': 'ᵉ', 'f': 'ᶠ', 'g': 'ᵍ', 'h': 'ʰ', 'i': 'ⁱ', 'j': 'ʲ',
            'k': 'ᵏ', 'l': 'ˡ', 'm': 'ᵐ', 'n': 'ⁿ', 'o': 'ᵒ', 'p': 'ᵖ', 'r': 'ʳ', 's': 'ˢ', 't': 'ᵗ', 'u': 'ᵘ',
            'v': 'ᵛ', 'w': 'ʷ', 'x': 'ˣ', 'y': 'ʸ', 'z': 'ᶻ',
            'A': 'ᴬ', 'B': 'ᴮ', 'D': 'ᴰ', 'E': 'ᴱ', 'G': 'ᴳ', 'H': 'ᴴ', 'I': 'ᴵ', 'J': 'ᴶ', 'K': 'ᴷ',
            'L': 'ᴸ', 'M': 'ᴹ', 'N': 'ᴺ', 'O': 'ᴼ', 'P': 'ᴾ', 'R': 'ᴿ', 'T': 'ᵀ', 'U': 'ᵁ', 'V': 'ⱽ', 'W': 'ᵂ',
            '+': '⁺', '-': '⁻', '=': '⁼', '(': '⁽', ')': '⁾'
        }
    
    def parse(self, mathml: str) -> MathBox:
        """Parse MathML string and return rendered ASCII box."""
        # Clean up the MathML string
        mathml = mathml.strip()
        
        # Parse XML
        try:
            root = ET.fromstring(mathml)
        except ET.ParseError:
            # Try wrapping in math tags if not present
            if not mathml.startswith('<math'):
                mathml = f'<math xmlns="http://www.w3.org/1998/Math/MathML">{mathml}</math>'
                root = ET.fromstring(mathml)
            else:
                raise
        
        # Process the root element
        return self.process_element(root)
    
    def process_element(self, elem: ET.Element) -> MathBox:
        """Process a MathML element and return its rendered box."""
        # Remove namespace prefix for easier handling
        tag = elem.tag.split('}')[-1] if '}' in elem.tag else elem.tag
        
        if tag == 'math':
            # Process the content of math element
            if len(elem) > 0:
                return self.process_element(elem[0])
            else:
                return MathBox(elem.text or '')
        
        elif tag == 'mrow':
            # Horizontal group
            return self.process_mrow(elem)
        
        elif tag == 'mi':
            # Identifier (variable)
            return MathBox(elem.text or '')
        
        elif tag == 'mo':
            # Operator
            text = elem.text or ''
            # Add spacing around binary operators
            if text in ['=', '+', '-', '*', '/']:
                return MathBox(f' {text} ')
            # No extra spacing for brackets, parentheses
            elif text in ['(', ')', '[', ']', '{', '}']:
                return MathBox(text)
            # Summation and other special operators
            else:
                return MathBox(text)
        
        elif tag == 'mfrac':
            # Fraction
            return self.process_fraction(elem)
        
        elif tag == 'msub':
            # Subscript
            return self.process_subscript(elem)
        
        elif tag == 'msup':
            # Superscript
            return self.process_superscript(elem)
        
        elif tag == 'munder':
            # Under (like sum with subscript)
            return self.process_under(elem)
        
        else:
            # Default: concatenate children horizontally
            if len(elem) > 0:
                boxes = [self.process_element(child) for child in elem]
                return self.horizontal_concat(boxes)
            else:
                return MathBox(elem.text or '')
    
    def process_mrow(self, elem: ET.Element) -> MathBox:
        """Process an mrow (horizontal group) element."""
        boxes = []
        
        # Process text before first child
        if elem.text and elem.text.strip():
            boxes.append(MathBox(elem.text.strip()))
        
        # Process children
        for child in elem:
            child_box = self.process_element(child)
            if child_box.width > 0:  # Only add non-empty boxes
                boxes.append(child_box)
            # Process text after each child (tail)
            if child.tail and child.tail.strip():
                boxes.append(MathBox(child.tail.strip()))
        
        if not boxes:
            return MathBox()
        
        return self.horizontal_concat(boxes)
    
    def process_fraction(self, elem: ET.Element) -> MathBox:
        """Process a fraction element."""
        if len(elem) != 2:
            return MathBox('?')
        
        numerator = self.process_element(elem[0])
        denominator = self.process_element(elem[1])
        
        # Calculate dimensions
        width = max(numerator.width, denominator.width)
        height = numerator.height + 1 + denominator.height
        baseline = numerator.height  # Fraction bar at baseline
        
        # Create result box
        result = MathBox.create_empty(width, height, baseline)
        
        # Place numerator (centered, above fraction bar)
        num_offset = (width - numerator.width) // 2
        for y in range(numerator.height):
            for x in range(numerator.width):
                result.set_char(x + num_offset, y, numerator.get_char(x, y))
        
        # Draw fraction bar at baseline
        for x in range(width):
            result.set_char(x, baseline, '─')
        
        # Place denominator (centered, below fraction bar)
        den_offset = (width - denominator.width) // 2
        for y in range(denominator.height):
            for x in range(denominator.width):
                result.set_char(x + den_offset, baseline + 1 + y, denominator.get_char(x, y))
        
        return result
    
    def try_unicode_subscript(self, text: str) -> Optional[str]:
        """Try to convert text to Unicode subscripts, return None if not possible."""
        if not self.use_unicode or not text:
            return None
        
        result = ""
        for char in text:
            if char in self.unicode_subscripts:
                result += self.unicode_subscripts[char]
            else:
                return None  # Can't convert this character
        return result
    
    def try_unicode_superscript(self, text: str) -> Optional[str]:
        """Try to convert text to Unicode superscripts, return None if not possible."""
        if not self.use_unicode or not text:
            return None
        
        # Special heuristic: for single-character common notation, use inline
        if len(text) == 1 and text in ["'", '"', '*']:
            return text  # Keep prime, double prime, asterisk inline
        
        result = ""
        for char in text:
            if char in self.unicode_superscripts:
                result += self.unicode_superscripts[char]
            else:
                return None  # Can't convert this character
        return result

    def process_subscript(self, elem: ET.Element) -> MathBox:
        """Process a subscript element."""
        if len(elem) != 2:
            return MathBox('?')
        
        base = self.process_element(elem[0])
        subscript = self.process_element(elem[1])
        
        # Try Unicode subscript first if both base and subscript are simple text
        if (base.height == 1 and base.baseline == 0 and 
            subscript.height == 1 and subscript.baseline == 0):
            subscript_text = ''.join(subscript.content[0]).strip()
            unicode_sub = self.try_unicode_subscript(subscript_text)
            
            if unicode_sub:
                # Use Unicode subscript - single line
                base_text = ''.join(base.content[0]).strip()
                combined_text = base_text + unicode_sub
                return MathBox(combined_text)
        
        # Fall back to multiline positioning
        width = base.width + subscript.width
        height = max(base.height, base.baseline + 1 + subscript.height)
        baseline = base.baseline
        
        # Create result box
        result = MathBox.create_empty(width, height, baseline)
        
        # Place base
        for y in range(base.height):
            for x in range(base.width):
                result.set_char(x, y, base.get_char(x, y))
        
        # Place subscript (below and to the right)
        sub_y_offset = base.baseline + 1
        for y in range(subscript.height):
            for x in range(subscript.width):
                if sub_y_offset + y < height:
                    result.set_char(base.width + x, sub_y_offset + y, subscript.get_char(x, y))
        
        return result
    
    def process_superscript(self, elem: ET.Element) -> MathBox:
        """Process a superscript element."""
        if len(elem) != 2:
            return MathBox('?')
        
        base = self.process_element(elem[0])
        superscript = self.process_element(elem[1])
        
        # Try Unicode superscript first if both base and superscript are simple text
        if (base.height == 1 and base.baseline == 0 and 
            superscript.height == 1 and superscript.baseline == 0):
            superscript_text = ''.join(superscript.content[0]).strip()
            unicode_sup = self.try_unicode_superscript(superscript_text)
            
            if unicode_sup:
                # Use Unicode superscript - single line
                base_text = ''.join(base.content[0]).strip()
                combined_text = base_text + unicode_sup
                return MathBox(combined_text)
        
        # Fall back to multiline positioning
        width = base.width + superscript.width
        height = superscript.height + base.height
        baseline = superscript.height + base.baseline
        
        # Create result box
        result = MathBox.create_empty(width, height, baseline)
        
        # Place superscript (above and to the right of base)
        for y in range(superscript.height):
            for x in range(superscript.width):
                result.set_char(base.width + x, y, superscript.get_char(x, y))
        
        # Place base (below superscript)
        for y in range(base.height):
            for x in range(base.width):
                result.set_char(x, superscript.height + y, base.get_char(x, y))
        
        return result
    
    def process_under(self, elem: ET.Element) -> MathBox:
        """Process an under element (like summation with subscript)."""
        if len(elem) != 2:
            return MathBox('?')
        
        base = self.process_element(elem[0])
        under = self.process_element(elem[1])
        
        # Check if this is a summation - if so, add some spacing
        is_summation = (len(elem[0].text or '') > 0 and '∑' in elem[0].text) if hasattr(elem[0], 'text') else False
        
        # Calculate dimensions
        width = max(base.width, under.width)
        if is_summation:
            width = max(width, 2)  # Ensure minimum width for summation
        height = base.height + under.height
        baseline = base.baseline
        
        # Create result box
        result = MathBox.create_empty(width, height, baseline)
        
        # Place base (centered if needed)
        base_offset = (width - base.width) // 2
        for y in range(base.height):
            for x in range(base.width):
                result.set_char(x + base_offset, y, base.get_char(x, y))
        
        # Place under (centered below)
        under_offset = (width - under.width) // 2
        for y in range(under.height):
            for x in range(under.width):
                result.set_char(x + under_offset, base.height + y, under.get_char(x, y))
        
        return result
    
    def horizontal_concat(self, boxes: List[MathBox]) -> MathBox:
        """Concatenate boxes horizontally, aligning at baseline."""
        # Filter out empty boxes
        boxes = [b for b in boxes if b.width > 0 and b.height > 0]
        
        if not boxes:
            return MathBox()
        
        if len(boxes) == 1:
            return boxes[0]
        
        # Calculate dimensions
        width = sum(box.width for box in boxes)
        max_above = max(box.baseline for box in boxes)
        max_below = max(box.height - box.baseline for box in boxes)
        height = max_above + max_below
        baseline = max_above
        
        # Create result box
        result = MathBox.create_empty(width, height, baseline)
        
        # Place each box
        x_offset = 0
        for box in boxes:
            y_offset = baseline - box.baseline
            for y in range(box.height):
                for x in range(box.width):
                    char = box.get_char(x, y)
                    if char and char != ' ':  # Only copy non-empty chars
                        if 0 <= y + y_offset < height and x + x_offset < width:
                            result.set_char(x + x_offset, y + y_offset, char)
            x_offset += box.width
        
        return result


def mathml_to_ascii(html: str, use_unicode: bool = True) -> str:
    """Convert HTML with MathML to ASCII representation.
    
    Args:
        html: HTML string containing MathML
        use_unicode: If True, use Unicode subscripts/superscripts when possible.
                    If False, always use multiline positioning.
    """
    parser = MathMLParser(use_unicode=use_unicode)
    
    # Extract MathML from HTML
    math_pattern = r'<math[^>]*>.*?</math>'
    matches = re.findall(math_pattern, html, re.DOTALL)
    
    if not matches:
        # No math elements found - return original HTML
        return html
    
    # For now, just process the first math element found
    # In a full implementation, we'd replace them in the HTML
    mathml = matches[0]
    box = parser.parse(mathml)
    return box.render()


def main():
    """Example usage."""
    # Example MathML
    mathml = """
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mfrac>
            <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
            <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
        </mfrac>
    </math>
    """
    
    result = mathml_to_ascii(mathml)
    print(result)


if __name__ == "__main__":
    main()