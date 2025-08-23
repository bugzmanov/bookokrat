#!/usr/bin/env python3
"""Test the normalization expression."""

from mathml_to_ascii import mathml_to_ascii

# The normalization expression
mathml = '''<math xmlns="http://www.w3.org/1998/Math/MathML" alttext="x prime equals StartFraction x minus min left-parenthesis x right-parenthesis Over max left-parenthesis x right-parenthesis minus min left-parenthesis x right-parenthesis EndFraction">
  <mrow>
    <mi>x</mi>
    <mi>â</mi>
    <mi></mi>
    <mi></mi>
    <mo>=</mo>
    <mfrac><mrow><mi>x</mi><mo>-</mo><mo movablelimits="true" form="prefix">min</mo><mo>(</mo><mi>x</mi><mo>)</mo></mrow> <mrow><mo movablelimits="true" form="prefix">max</mo><mo>(</mo><mi>x</mi><mo>)</mo><mo>-</mo><mo movablelimits="true" form="prefix">min</mo><mo>(</mo><mi>x</mi><mo>)</mo></mrow></mfrac>
  </mrow>
</math>'''

print("Normalization Expression:")
print("=" * 60)
result = mathml_to_ascii(mathml)
print(result)
print("=" * 60)

# Also show the alttext for comparison
print("\nAlt text says: x prime equals (x - min(x)) / (max(x) - min(x))")
print("\nNote: The 'â' character appears to be a rendering issue for the prime symbol")