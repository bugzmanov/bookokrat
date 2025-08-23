#!/usr/bin/env python3
"""Test the complex equation rendering."""

from mathml_to_ascii import mathml_to_ascii

# The complex equation from the user's example
html = '''
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
'''

print("Complex Equation Rendering:")
print("=" * 60)
result = mathml_to_ascii(html)
print(result)
print("=" * 60)

# Also test a simpler fraction alignment
simple = '''
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
'''

print("\nSimple Fraction Alignment Test:")
print("=" * 60)
result2 = mathml_to_ascii(simple)
print(result2)
print("=" * 60)

super_complex='''
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
'''

print("\nSuper complex test:")
print("=" * 60)
result3 = mathml_to_ascii(super_complex)
print(result3)
print("=" * 60)


duper = '''
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
'''

print("\nDuper complex test:")
print("=" * 60)
result4 = mathml_to_ascii(duper)
print(result4)
print("=" * 60)

# Square root test examples
sqrt_simple = '''
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <msqrt>
        <mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow>
    </msqrt>
</math>
'''

print("\nSimple Square Root Test:")
print("=" * 60)
result5 = mathml_to_ascii(sqrt_simple)
print(result5)
print("=" * 60)

# Complex square root with fraction
sqrt_complex = '''
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <msqrt>
        <mfrac>
            <mrow><msup><mi>x</mi><mn>2</mn></msup><mo>+</mo><msub><mi>b</mi><mi>c</mi></msub></mrow>
            <mrow><mi>sin</mi><mo>(</mo><mi>x</mi><mo>)</mo><msup><mi>cos</mi><mrow><mo>(</mo><mi>y</mi><mo>)</mo></mrow></msup><mo>+</mo><msup><mi>e</mi><mrow><mi>z</mi><mo>⋅</mo><mn>5</mn></mrow></msup></mrow>
        </mfrac>
    </msqrt>
</math>
'''

print("\nComplex Square Root with Fraction:")
print("=" * 60)
result6 = mathml_to_ascii(sqrt_complex)
print(result6)
print("=" * 60)

# Nested square roots
sqrt_nested = '''
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <msqrt>
        <mrow>
            <mi>a</mi>
            <mo>+</mo>
            <msqrt>
                <mrow><mi>b</mi><mo>+</mo><mi>c</mi></mrow>
            </msqrt>
        </mrow>
    </msqrt>
</math>
'''

print("\nNested Square Roots Test:")
print("=" * 60)
result7 = mathml_to_ascii(sqrt_nested)
print(result7)
print("=" * 60)
