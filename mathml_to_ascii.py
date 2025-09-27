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
            '+': '₊', '-': '₋', '=': '₌', '(': '₍', ')': '₎', ',': ',', ' ': ' '
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
            # Check if it's a prefix operator (like log)
            form = elem.get('form', '')

            # Handle special operators
            if form == 'prefix' or text in ['log', 'ln', 'sin', 'cos', 'tan', 'exp']:
                # Prefix operators don't get extra spacing
                return MathBox(text)
            # Add spacing around binary operators
            elif text in ['=', '+', '-', '*', '/', '≠']:  # ≠ is ≠ (not equal)
                return MathBox(f' {text} ')
            # No extra spacing for brackets, parentheses
            elif text in ['(', ')', '[', ']', '{', '}']:
                return MathBox(text)
            # Summation and other special operators
            else:
                return MathBox(text)

        elif tag == 'mn':
            # Number
            return MathBox(elem.text or '')

        elif tag == 'mtext':
            # Text content
            return MathBox(elem.text or '')

        elif tag == 'mspace':
            # Space element - return single space
            return MathBox(' ')
        
        elif tag == 'mfrac':
            # Fraction
            return self.process_fraction(elem)
        
        elif tag == 'msub':
            # Subscript
            return self.process_subscript(elem)
        
        elif tag == 'msup':
            # Superscript
            return self.process_superscript(elem)

        elif tag == 'msubsup':
            # Both subscript and superscript
            return self.process_subsup(elem)

        elif tag == 'munder':
            # Under (like sum with subscript)
            return self.process_under(elem)

        elif tag == 'munderover':
            # Under and over (like sum with both sub and superscript)
            return self.process_underover(elem)

        elif tag == 'msqrt':
            # Square root
            return self.process_square_root(elem)

        elif tag == 'mroot':
            # Nth root
            return self.process_nth_root(elem)

        elif tag == 'mtable':
            # Table
            return self.process_table(elem)

        elif tag == 'mtr':
            # Table row
            return self.process_table_row(elem)

        elif tag == 'mtd':
            # Table data/cell
            return self.process_table_cell(elem)

        elif tag == 'mfenced':
            # Fenced expression (with braces, brackets, etc.)
            return self.process_fenced(elem)
        
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
        prev_child_tag = None
        for child in elem:
            # Get the tag name
            child_tag = child.tag.split('}')[-1] if '}' in child.tag else child.tag

            # Add spacing between fraction and summation
            if prev_child_tag == 'mfrac' and child_tag in ['msubsup', 'munderover', 'munder', 'mover']:
                boxes.append(MathBox('  '))  # Add two spaces

            child_box = self.process_element(child)
            if child_box.width > 0:  # Only add non-empty boxes
                boxes.append(child_box)

            # Process text after each child (tail)
            if child.tail and child.tail.strip():
                boxes.append(MathBox(child.tail.strip()))

            prev_child_tag = child_tag

        if not boxes:
            return MathBox()

        return self.horizontal_concat(boxes)
    
    def process_fraction(self, elem: ET.Element) -> MathBox:
        """Process a fraction element."""
        if len(elem) != 2:
            return MathBox('?')

        # Check for invisible fraction (linethickness="0pt") - used for conditions in summations
        linethickness = elem.get('linethickness', '')
        is_invisible = linethickness == '0pt'

        numerator = self.process_element(elem[0])
        denominator = self.process_element(elem[1])

        # For invisible fractions, stack vertically without a line
        if is_invisible:
            width = max(numerator.width, denominator.width)
            height = numerator.height + denominator.height
            baseline = numerator.height - 1  # Adjust baseline

            # Create result box
            result = MathBox.create_empty(width, height, baseline)

            # Place numerator (centered, top)
            num_offset = (width - numerator.width) // 2
            for y in range(numerator.height):
                for x in range(numerator.width):
                    result.set_char(x + num_offset, y, numerator.get_char(x, y))

            # Place denominator directly below (no bar)
            den_offset = (width - denominator.width) // 2
            for y in range(denominator.height):
                for x in range(denominator.width):
                    result.set_char(x + den_offset, numerator.height + y, denominator.get_char(x, y))

            return result

        # Regular fraction with visible bar
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

        # Try to convert all characters to Unicode subscripts
        result = ""
        for char in text:
            if char in self.unicode_subscripts:
                result += self.unicode_subscripts[char]
            else:
                # If any character can't be converted, use underscore format
                return f"_{text}"
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
    
    def process_subsup(self, elem: ET.Element) -> MathBox:
        """Process an element with both subscript and superscript."""
        if len(elem) != 3:
            return MathBox('?')

        base = self.process_element(elem[0])
        subscript = self.process_element(elem[1])
        superscript = self.process_element(elem[2])

        # Check if this is a summation operator
        base_text = ''.join(''.join(row) for row in base.content).strip()
        is_summation = '∑' in base_text

        if is_summation:
            # For summations, stack super/base/sub vertically and center
            width = max(base.width, subscript.width, superscript.width)
            height = superscript.height + base.height + subscript.height
            baseline = superscript.height + base.baseline

            # Create result box
            result = MathBox.create_empty(width, height, baseline)

            # Place superscript (centered above)
            super_offset = (width - superscript.width) // 2
            for y in range(superscript.height):
                for x in range(superscript.width):
                    result.set_char(x + super_offset, y, superscript.get_char(x, y))

            # Place base (centered in middle)
            base_offset = (width - base.width) // 2
            for y in range(base.height):
                for x in range(base.width):
                    result.set_char(x + base_offset, superscript.height + y, base.get_char(x, y))

            # Place subscript (centered below)
            sub_offset = (width - subscript.width) // 2
            for y in range(subscript.height):
                for x in range(subscript.width):
                    result.set_char(x + sub_offset, superscript.height + base.height + y, subscript.get_char(x, y))

            return result
        else:
            # For regular base with both sub and superscript, arrange diagonally
            # Try Unicode if possible for simple cases
            if (base.height == 1 and subscript.height == 1 and superscript.height == 1):
                base_text = ''.join(base.content[0]).strip()
                sub_text = ''.join(subscript.content[0]).strip()
                sup_text = ''.join(superscript.content[0]).strip()

                unicode_sub = self.try_unicode_subscript(sub_text)
                unicode_sup = self.try_unicode_superscript(sup_text)

                if unicode_sub and unicode_sup:
                    return MathBox(base_text + unicode_sub + unicode_sup)

            # Fall back to multiline positioning
            width = base.width + max(subscript.width, superscript.width)
            height = superscript.height + base.height + subscript.height
            baseline = superscript.height + base.baseline

            # Create result box
            result = MathBox.create_empty(width, height, baseline)

            # Place base
            for y in range(base.height):
                for x in range(base.width):
                    result.set_char(x, superscript.height + y, base.get_char(x, y))

            # Place superscript (to the right and above)
            for y in range(superscript.height):
                for x in range(superscript.width):
                    result.set_char(base.width + x, y, superscript.get_char(x, y))

            # Place subscript (to the right and below)
            sub_y_offset = superscript.height + base.height
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

    def process_underover(self, elem: ET.Element) -> MathBox:
        """Process an underover element (like summation with both subscript and superscript)."""
        if len(elem) != 3:
            return MathBox('?')

        base = self.process_element(elem[0])
        under = self.process_element(elem[1])
        over = self.process_element(elem[2])

        # Check if this is a summation
        is_summation = (len(elem[0].text or '') > 0 and '∑' in elem[0].text) if hasattr(elem[0], 'text') else False

        # Calculate dimensions
        width = max(base.width, under.width, over.width)
        if is_summation:
            width = max(width, 2)  # Ensure minimum width for summation
        height = over.height + base.height + under.height
        baseline = over.height + base.baseline

        # Create result box
        result = MathBox.create_empty(width, height, baseline)

        # Place over (centered above)
        over_offset = (width - over.width) // 2
        for y in range(over.height):
            for x in range(over.width):
                result.set_char(x + over_offset, y, over.get_char(x, y))

        # Place base (centered in middle)
        base_offset = (width - base.width) // 2
        for y in range(base.height):
            for x in range(base.width):
                result.set_char(x + base_offset, over.height + y, base.get_char(x, y))

        # Place under (centered below)
        under_offset = (width - under.width) // 2
        for y in range(under.height):
            for x in range(under.width):
                result.set_char(x + under_offset, over.height + base.height + y, under.get_char(x, y))

        return result

    def generate_sqrt_radical(self, height, length):
        """Generate square root radical symbol with given height and length."""
        if height < 3:
            height = 3  # Minimum height
        
        lines = []
        
        # Top line: overline with diagonal start
        top_padding = height + 1  # Space before the overline
        overline = "⟋" + "─" * length
        lines.append(" " * top_padding + overline)
        
        # Middle diagonal lines
        for i in range(1, height - 2):
            padding = height + 1 - i
            lines.append(" " * padding + "╱  ")
        
        # Second to last line: connecting part
        if height > 2:
            lines.append("_  ╱  ")
        
        # Last line: tail
        lines.append(" \\╱  ")
        
        return lines

    def process_square_root(self, elem: ET.Element) -> MathBox:
        """Process a square root element."""
        if len(elem) == 0:
            return MathBox('√')
        
        # Step 1: Generate the inner formula content
        if len(elem) == 1:
            inner = self.process_element(elem[0])
        else:
            # Multiple children - treat as horizontal group
            boxes = [self.process_element(child) for child in elem]
            inner = self.horizontal_concat(boxes)
        
        # For single line expressions, use simple format
        if inner.height == 1:
            inner_text = ''.join(inner.content[0]).strip()
            return MathBox(f'√({inner_text})')
        
        # Step 2: Measure the formula dimensions
        formula_width = inner.width
        formula_height = inner.height
        
        # Step 3: Generate the radical symbol using our function
        # Add 1 to height to account for the overline space
        radical_lines = self.generate_sqrt_radical(formula_height + 1, formula_width + 4)
        
        # Step 4: Calculate total dimensions
        radical_width = len(radical_lines[0]) if radical_lines else 0
        total_width = max(radical_width, formula_width + 10)  # Extra padding
        total_height = len(radical_lines)
        baseline = inner.baseline + 1
        
        # Create result box
        result = MathBox.create_empty(total_width, total_height, baseline)
        
        # Place the radical symbol
        for y, line in enumerate(radical_lines):
            for x, char in enumerate(line):
                if x < total_width and char != ' ':
                    result.set_char(x, y, char)
        
        # Step 5: Place the formula content in the space under the overline
        # Content should start after the diagonal space
        content_x_offset = formula_height + 3  # Space for diagonal + padding
        content_y_offset = 1  # Below the overline
        
        for y in range(inner.height):
            for x in range(inner.width):
                char = inner.get_char(x, y)
                if char and char != ' ':
                    target_x = content_x_offset + x
                    target_y = content_y_offset + y
                    if target_x < total_width and target_y < total_height:
                        result.set_char(target_x, target_y, char)
        
        return result

    def process_nth_root(self, elem: ET.Element) -> MathBox:
        """Process an nth root element (mroot)."""
        if len(elem) != 2:
            return MathBox('?')

        # First child is the radicand (what's under the root)
        # Second child is the root index (the n in nth root)
        radicand = self.process_element(elem[0])
        index = self.process_element(elem[1])

        # For single line expressions with simple index, use Unicode format
        if radicand.height == 1 and index.height == 1:
            radicand_text = ''.join(radicand.content[0]).strip()
            index_text = ''.join(index.content[0]).strip()

            # Try to convert index to superscript
            unicode_index = self.try_unicode_superscript(index_text)

            if unicode_index:
                # Use Unicode format: ³√x for cube root
                return MathBox(f'{unicode_index}√({radicand_text})')
            else:
                # Fallback to notation like: [3]√(x)
                return MathBox(f'[{index_text}]√({radicand_text})')

        # For multi-line expressions, create ASCII art with proper alignment
        formula_width = radicand.width
        formula_height = radicand.height
        index_text = ''.join(''.join(row) for row in index.content).strip()
        index_width = len(index_text)

        # Generate radical lines for nth root
        # We need exactly 5 lines for a fraction:
        # 1. Overline
        # 2. Diagonal for numerator
        # 3. Diagonal for fraction bar
        # 4. Diagonal with underscore for denominator (where index goes)
        # 5. Bottom tail

        lines = []

        # Use the EXACT SAME structure as square root!
        # For fraction height 3, we need 4 lines total with proper padding
        if formula_height == 3:
            height = 4  # height + 1 for the overline, same as sqrt
            # Line 1: overline (padding = height + 1 = 5 spaces)
            lines.append(" " * 5 + "⟋" + "─" * (formula_width + 4))
            # Line 2: diagonal for numerator (padding = height + 1 - 1 = 4)
            lines.append(" " * 4 + "╱  ")
            # Line 3: underscore + diagonal (this is where index goes)
            lines.append("_  ╱  ")
            # Line 4: bottom tail
            lines.append(" \\╱  ")
        else:
            # For other heights, generate dynamically
            radical_line_height = formula_height + 2
            # Top line: overline with diagonal start
            top_padding = radical_line_height - 1
            overline = "⟋" + "─" * (formula_width + 4)
            lines.append(" " * top_padding + overline)

            # Generate diagonal lines
            for i in range(1, radical_line_height):
                padding = radical_line_height - 1 - i
                if i == radical_line_height - 1:
                    # Last line: tail at the bottom
                    lines.append(" " * padding + "\\╱  ")
                elif i == radical_line_height - 2:
                    # Second to last line: connecting part with underscore
                    lines.append("_" + " " * padding + "╱  ")
                else:
                    # Middle diagonal lines
                    lines.append(" " * (padding + 1) + "╱  ")

        # Now prepend the index to the appropriate line
        # The index should go on the line with the underscore "_"
        modified_lines = []
        for i, line in enumerate(lines):
            # Find the line that starts with underscore
            if line.lstrip().startswith("_"):
                # This is the line with underscore - add index here WITH A SPACE
                # The underscore line has no leading spaces, so add one space before index
                modified_lines.append(" " + index_text + line)
            else:
                # Other lines need to be padded by index width + 1 for alignment
                modified_lines.append(" " * (index_width + 1) + line)

        # Calculate total dimensions
        radical_width = max(len(line) for line in modified_lines) if modified_lines else 0
        total_width = max(radical_width, formula_width + 10 + index_width)
        # Ensure total height is at least tall enough for the radicand content
        # The radicand starts at y_offset=1 (below the overline)
        total_height = max(len(modified_lines), 1 + radicand.height)
        baseline = radicand.baseline + 1

        # Create result box
        result = MathBox.create_empty(total_width, total_height, baseline)

        # Place the radical symbol with index
        for y, line in enumerate(modified_lines):
            for x, char in enumerate(line):
                if x < total_width and char != ' ':
                    result.set_char(x, y, char)

        # Place the formula content under the overline
        # Use SAME offset logic as square root!
        if formula_height == 3:
            # sqrt uses formula_height + 3, we add index_width + 1 for the space
            content_x_offset = 7 + index_width  # 4 + 3 for sqrt logic, adjusted for index placement
        else:
            content_x_offset = formula_height + 3 + index_width + 1  # Adjusted for index
        content_y_offset = 1  # Below the overline

        for y in range(radicand.height):
            for x in range(radicand.width):
                char = radicand.get_char(x, y)
                if char and char != ' ':
                    target_x = content_x_offset + x
                    target_y = content_y_offset + y
                    if target_x < total_width and target_y < total_height:
                        result.set_char(target_x, target_y, char)

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

    def process_table(self, elem: ET.Element) -> MathBox:
        """Process a table element."""
        rows = []
        max_width = 0

        for child in elem:
            tag = child.tag.split('}')[-1] if '}' in child.tag else child.tag
            if tag == 'mtr':
                row_box = self.process_table_row(child)
                rows.append(row_box)
                max_width = max(max_width, row_box.width)

        if not rows:
            return MathBox()

        # Check if this is a table with "where" clause (common in mathematical definitions)
        has_where_clause = False
        if len(rows) >= 2:
            # Check if second row contains "where" text
            for child in elem:
                tag = child.tag.split('}')[-1] if '}' in child.tag else child.tag
                if tag == 'mtr':
                    for td in child:
                        td_tag = td.tag.split('}')[-1] if '}' in td.tag else td.tag
                        if td_tag == 'mtd':
                            for elem_child in td:
                                if hasattr(elem_child, 'text') and elem_child.text and 'where' in elem_child.text.lower():
                                    has_where_clause = True
                                    break

        # Calculate total height, adding space if we have a where clause
        total_height = sum(row.height for row in rows)
        if has_where_clause and len(rows) >= 2:
            total_height += 1  # Add empty line between main equation and where clause

        # Create result box with proper dimensions
        result = MathBox.create_empty(max_width, total_height, 0)

        # Place each row
        y_offset = 0
        for i, row in enumerate(rows):
            # Add empty line before "where" clause (second row)
            if has_where_clause and i == 1:
                y_offset += 1

            # Center align if row is narrower than max width
            x_offset = (max_width - row.width) // 2
            for y in range(row.height):
                for x in range(row.width):
                    char = row.get_char(x, y)
                    if char and char != ' ':
                        result.set_char(x + x_offset, y + y_offset, char)
            y_offset += row.height

        # Set baseline to middle of table
        result.baseline = total_height // 2

        return result

    def process_table_row(self, elem: ET.Element) -> MathBox:
        """Process a table row element."""
        cells = []

        for child in elem:
            tag = child.tag.split('}')[-1] if '}' in child.tag else child.tag
            if tag == 'mtd':
                cell_box = self.process_table_cell(child)
                cells.append(cell_box)

        if not cells:
            return MathBox()

        # Concatenate cells horizontally with spacing
        boxes_with_spacing = []
        for i, cell in enumerate(cells):
            boxes_with_spacing.append(cell)
            if i < len(cells) - 1:
                # Add spacing between cells
                boxes_with_spacing.append(MathBox('  '))

        return self.horizontal_concat(boxes_with_spacing)

    def process_table_cell(self, elem: ET.Element) -> MathBox:
        """Process a table cell element."""
        if len(elem) > 0:
            # Process cell content
            boxes = []
            for child in elem:
                box = self.process_element(child)
                if box.width > 0:
                    boxes.append(box)

            if boxes:
                return self.horizontal_concat(boxes)

        # Handle text content
        if elem.text:
            return MathBox(elem.text)

        return MathBox()

    def process_fenced(self, elem: ET.Element) -> MathBox:
        """Process a fenced expression (with braces, brackets, etc.)."""
        # Get opening and closing delimiters
        open_delim = elem.get('open', '(')
        close_delim = elem.get('close', ')')
        separators = elem.get('separators', ',')

        # Process the content
        if len(elem) > 0:
            # Process ALL children, not just the first one
            boxes = []
            for i, child in enumerate(elem):
                child_box = self.process_element(child)
                if child_box.width > 0:
                    boxes.append(child_box)
                    # Add separator between elements if specified
                    if i < len(elem) - 1 and separators:
                        # Only add separator if it's not empty
                        if separators != '':
                            boxes.append(MathBox(separators))

            if boxes:
                content_box = self.horizontal_concat(boxes)
            else:
                content_box = MathBox()
        else:
            content_box = MathBox()

        # Special handling for braces - need multi-line
        if open_delim == '{' and content_box.height > 1:
            return self.create_multi_line_brace(content_box)

        # For single-line content, just add delimiters
        if content_box.height == 1:
            result_text = open_delim + ''.join(content_box.content[0]) + close_delim
            return MathBox(result_text)

        # For multi-line content with parentheses/brackets
        return self.create_multi_line_delimiters(content_box, open_delim, close_delim)

    def create_multi_line_brace(self, content: MathBox) -> MathBox:
        """Create a multi-line brace around content."""
        height = content.height
        width = content.width + 2  # Space for brace

        result = MathBox.create_empty(width, height, content.baseline)

        # Draw left brace
        if height == 1:
            result.set_char(0, 0, '{')
        elif height == 2:
            result.set_char(0, 0, '⎧')
            result.set_char(0, 1, '⎩')
        elif height == 3:
            result.set_char(0, 0, '⎧')
            result.set_char(0, 1, '⎨')
            result.set_char(0, 2, '⎩')
        else:
            # For larger braces
            result.set_char(0, 0, '⎧')
            for i in range(1, height - 1):
                if i == height // 2:
                    result.set_char(0, i, '⎨')
                else:
                    result.set_char(0, i, '⎪')
            result.set_char(0, height - 1, '⎩')

        # Copy content
        for y in range(content.height):
            for x in range(content.width):
                char = content.get_char(x, y)
                if char and char != ' ':
                    result.set_char(x + 1, y, char)

        return result

    def create_multi_line_delimiters(self, content: MathBox, open_delim: str, close_delim: str) -> MathBox:
        """Create multi-line delimiters (parentheses, brackets) around content."""
        height = content.height
        width = content.width + 2  # Space for delimiters

        result = MathBox.create_empty(width, height, content.baseline)

        # Map delimiters to multi-line versions
        if open_delim == '(':
            if height == 2:
                result.set_char(0, 0, '⎛')
                result.set_char(0, 1, '⎝')
            else:
                result.set_char(0, 0, '⎛')
                for i in range(1, height - 1):
                    result.set_char(0, i, '⎜')
                result.set_char(0, height - 1, '⎝')
        elif open_delim == '[':
            if height == 2:
                result.set_char(0, 0, '⎡')
                result.set_char(0, 1, '⎣')
            else:
                result.set_char(0, 0, '⎡')
                for i in range(1, height - 1):
                    result.set_char(0, i, '⎢')
                result.set_char(0, height - 1, '⎣')

        # Copy content
        for y in range(content.height):
            for x in range(content.width):
                char = content.get_char(x, y)
                if char and char != ' ':
                    result.set_char(x + 1, y, char)

        # Add closing delimiter
        if close_delim == ')':
            if height == 2:
                result.set_char(width - 1, 0, '⎞')
                result.set_char(width - 1, 1, '⎠')
            else:
                result.set_char(width - 1, 0, '⎞')
                for i in range(1, height - 1):
                    result.set_char(width - 1, i, '⎟')
                result.set_char(width - 1, height - 1, '⎠')
        elif close_delim == ']':
            if height == 2:
                result.set_char(width - 1, 0, '⎤')
                result.set_char(width - 1, 1, '⎦')
            else:
                result.set_char(width - 1, 0, '⎤')
                for i in range(1, height - 1):
                    result.set_char(width - 1, i, '⎥')
                result.set_char(width - 1, height - 1, '⎦')

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
    # Example MathML with square root
    mathml = """
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <msqrt>
            <mfrac>
                <mrow><msup><mi>x</mi><mn>2</mn></msup><mo>+</mo><msub><mi>b</mi><mi>c</mi></msub></mrow>
                <mrow><mi>sin</mi><mo>(</mo><mi>x</mi><mo>)</mo><msup><mi>cos</mi><mrow><mo>(</mo><mi>y</mi><mo>)</mo></mrow></msup><mo>+</mo><msup><mi>e</mi><mrow><mi>z</mi><mo>⋅</mo><mn>5</mn></mrow></msup></mrow>
            </mfrac>
        </msqrt>
    </math>
    """
    
    result = mathml_to_ascii(mathml)
    print("Square root example:")
    print(result)
    print()
    
    # Simple square root example
    simple_sqrt = """
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <msqrt>
            <mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow>
        </msqrt>
    </math>
    """
    
    result2 = mathml_to_ascii(simple_sqrt)
    print("Simple square root:")
    print(result2)
    print()

    # Complex table example with fenced expression
    complex_table = """
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
    """

    result3 = mathml_to_ascii(complex_table)
    print("Complex table with fenced expression:")
    print(result3)
    print()

    # Summation with underover example (entropy formula)
    entropy_formula = """
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
    """

    result4 = mathml_to_ascii(entropy_formula)
    print("Entropy formula with summation:")
    print(result4)
    print()

    # Test nth root examples
    cube_root = """
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mroot>
            <mn>8</mn>
            <mn>3</mn>
        </mroot>
    </math>
    """

    result_cube = mathml_to_ascii(cube_root)
    print("Cube root of 8:")
    print(result_cube)
    print()

    # Fourth root with expression
    fourth_root = """
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
    """

    result_fourth = mathml_to_ascii(fourth_root)
    print("Fourth root of (x² + 1):")
    print(result_fourth)
    print()

    # Nth root with variable index
    nth_root = """
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mroot>
            <mi>a</mi>
            <mi>n</mi>
        </mroot>
    </math>
    """

    result_nth = mathml_to_ascii(nth_root)
    print("Nth root of a:")
    print(result_nth)
    print()

    # Complex nth root with fraction inside
    complex_nth_root = """
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
    """

    result_complex_nth = mathml_to_ascii(complex_nth_root)
    print("Fifth root of fraction:")
    print(result_complex_nth)
    print()

    # RMSE formula example
    rmse_formula = """
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
    """

    result5 = mathml_to_ascii(rmse_formula)
    print("RMSE formula:")
    print(result5)


if __name__ == "__main__":
    main()
