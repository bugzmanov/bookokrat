# MathML to ASCII Converter

A Python program that converts MathML expressions to ASCII art for terminal display.

## Features

- **No hardcoding** - Fully algorithmic approach
- **Proper baseline alignment** - Fractions align with equals signs
- **Support for common MathML elements**:
  - Fractions (`<mfrac>`)
  - Subscripts (`<msub>`)
  - Summations with subscripts (`<munder>`)
  - Horizontal grouping (`<mrow>`)
  - Variables (`<mi>`)
  - Operators (`<mo>`)
  - Nested expressions

## Algorithm

The converter uses a **box model** approach:

1. **Each MathML element becomes a `MathBox`** with:
   - Width and height in characters
   - Baseline position (for vertical alignment)
   - 2D character grid for content

2. **Vertical alignment rules**:
   - Fraction bars align with the baseline (same level as = signs)
   - Numerators go above the baseline
   - Denominators go below the baseline
   - Subscripts go below their base element

3. **Horizontal concatenation**:
   - Elements are aligned by their baselines
   - Spacing is added around binary operators

4. **Recursive processing**:
   - Complex expressions are built by recursively processing child elements
   - Parent elements combine child boxes according to mathematical rules

## Usage

```python
from mathml_to_ascii import mathml_to_ascii

mathml = '''
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <mfrac>
        <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
        <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
    </mfrac>
</math>
'''

result = mathml_to_ascii(mathml)
print(result)
# Output:
# P(x)
# ────
# Q(x)
```

## Examples

### Simple Fraction
```
k
─
n
```

### Complex Expression
```
                            P(x)          P(x) 
E    [x] = ∑ P(x)x = ∑ Q(x)x──── = E    [x────]
 P(x)      x         x      Q(x)    Q(x)  Q(x) 
```

### Multiple Fractions with Alignment
```
    a + b     d  
y = ───── + ─────
      c     e - f
```

## Testing

Run the test suite:
```bash
python3 test_mathml_to_ascii.py
```

Run the demo with various examples:
```bash
python3 demo_mathml.py
```

## Files

- `mathml_to_ascii.py` - Main converter implementation
- `test_mathml_to_ascii.py` - Comprehensive test suite (16 tests)
- `demo_mathml.py` - Demo with various MathML examples
- `test_complex_equation.py` - Test for the specific complex equation

## Limitations

- Currently processes only the first `<math>` element in HTML
- Some advanced MathML features not yet supported (superscripts, matrices, etc.)
- Terminal constraints mean perfect typesetting isn't possible

## Implementation Notes

The key insight is that mathematical expressions have a **baseline** concept - the imaginary line where most characters sit. In our implementation:

- Equals signs, plus signs, and variables sit on the baseline
- Fraction bars also sit on the baseline
- Subscripts drop below the baseline
- The algorithm maintains proper baseline alignment when concatenating elements horizontally

This approach ensures that expressions like `y = a/b + c/d` render with the fraction bars aligned with the equals sign, just as they would in proper mathematical typesetting.