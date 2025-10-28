#!/usr/bin/env python3
"""Demo of MathML to ASCII conversion with various examples."""

from mathml_to_ascii import mathml_to_ascii

examples = [
    ("Simple Fraction", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mfrac>
            <mi>k</mi>
            <mi>n</mi>
        </mfrac>
    </math>
    '''),
    
    ("Fraction with expressions", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mfrac>
            <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
            <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
        </mfrac>
    </math>
    '''),
    
    ("Equation with multiple fractions", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mrow>
            <mi>y</mi>
            <mo>=</mo>
            <mfrac>
                <mrow><mi>a</mi><mo>+</mo><mi>b</mi></mrow>
                <mi>c</mi>
            </mfrac>
            <mo>+</mo>
            <mfrac>
                <mi>d</mi>
                <mrow><mi>e</mi><mo>-</mo><mi>f</mi></mrow>
            </mfrac>
        </mrow>
    </math>
    '''),
    
    ("Subscripts", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mrow>
            <msub><mi>E</mi><mi>x</mi></msub>
            <mo>=</mo>
            <msub><mi>m</mi><mi>0</mi></msub>
            <msub><mi>c</mi><mi>2</mi></msub>
        </mrow>
    </math>
    '''),
    
    ("Summation", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mrow>
            <munder>
                <mo>∑</mo>
                <mrow><mi>i</mi><mo>=</mo><mi>1</mi></mrow>
            </munder>
            <msub><mi>x</mi><mi>i</mi></msub>
        </mrow>
    </math>
    '''),
    
    ("Complex nested expression", '''
    <math xmlns="http://www.w3.org/1998/Math/MathML">
        <mrow>
            <mi>f</mi><mo>(</mo><mi>x</mi><mo>)</mo>
            <mo>=</mo>
            <mfrac>
                <mn>1</mn>
                <mrow>
                    <mn>1</mn>
                    <mo>+</mo>
                    <mfrac>
                        <mn>1</mn>
                        <mi>x</mi>
                    </mfrac>
                </mrow>
            </mfrac>
        </mrow>
    </math>
    '''),
    
    ("The complex equation", '''
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
    ''')
]

for title, mathml in examples:
    print(f"\n{title}:")
    print("=" * 60)
    result = mathml_to_ascii(mathml)
    print(result)
    print("=" * 60)