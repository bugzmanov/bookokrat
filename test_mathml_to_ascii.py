#!/usr/bin/env python3
"""
Tests for MathML to ASCII converter.
"""

from mathml_to_ascii import MathBox, MathMLParser, mathml_to_ascii


class TestMathBox:
    """Test the MathBox class."""
    
    def test_simple_text_box(self):
        box = MathBox("hello")
        assert box.width == 5
        assert box.height == 1
        assert box.baseline == 0
        assert box.render() == "hello"
    
    def test_empty_box(self):
        box = MathBox.create_empty(3, 2, 1)
        assert box.width == 3
        assert box.height == 2
        assert box.baseline == 1
        assert box.render() == "   \n   "
    
    def test_get_set_char(self):
        box = MathBox.create_empty(3, 2, 0)
        box.set_char(1, 0, 'X')
        assert box.get_char(1, 0) == 'X'
        assert box.get_char(10, 10) == ' '  # Out of bounds


class TestMathMLParser:
    """Test the MathML parser."""
    
    def test_simple_variable(self):
        parser = MathMLParser()
        mathml = '<mi>x</mi>'
        box = parser.parse(mathml)
        assert box.render() == 'x'
    
    def test_simple_fraction(self):
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mfrac>
                <mi>a</mi>
                <mi>b</mi>
            </mfrac>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        assert 'a' in result
        assert '─' in result
        assert 'b' in result
        lines = result.split('\n')
        assert len(lines) == 3
        assert 'a' in lines[0]
        assert '─' in lines[1]
        assert 'b' in lines[2]
    
    def test_complex_fraction(self):
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mfrac>
                <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
                <mrow><mi>Q</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
            </mfrac>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        lines = result.split('\n')
        assert len(lines) == 3
        assert 'P(x)' in lines[0]
        assert '────' in lines[1]
        assert 'Q(x)' in lines[2]
    
    def test_subscript(self):
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <msub>
                <mi>E</mi>
                <mi>x</mi>
            </msub>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        lines = result.split('\n')
        assert len(lines) >= 2
        assert 'E' in lines[0]
        assert 'x' in lines[1]
    
    def test_summation_with_subscript(self):
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <munder>
                <mo>∑</mo>
                <mi>x</mi>
            </munder>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        lines = result.split('\n')
        assert '∑' in lines[0]
        assert 'x' in lines[1]
    
    def test_horizontal_concatenation(self):
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mrow>
                <mi>a</mi>
                <mo>=</mo>
                <mi>b</mi>
            </mrow>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        assert 'a = b' in result or 'a=b' in result
    
    def test_complex_expression(self):
        """Test the complex expression from the example."""
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mrow>
                <msub>
                    <mi>E</mi>
                    <mrow><mi>P</mi><mo>(</mo><mi>x</mi><mo>)</mo></mrow>
                </msub>
                <mrow>
                    <mo>[</mo>
                    <mi>x</mi>
                    <mo>]</mo>
                </mrow>
                <mo>=</mo>
                <munder><mo>∑</mo><mi>x</mi></munder>
                <mi>P</mi>
                <mrow>
                    <mo>(</mo>
                    <mi>x</mi>
                    <mo>)</mo>
                </mrow>
                <mi>x</mi>
            </mrow>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        # Check key components are present
        assert 'E' in result
        assert 'P(x)' in result
        assert '[x]' in result
        assert '=' in result
        assert '∑' in result
    
    def test_equation_with_fractions(self):
        """Test equation with multiple fractions aligned."""
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mrow>
                <mi>y</mi>
                <mo>=</mo>
                <mfrac>
                    <mi>a</mi>
                    <mi>b</mi>
                </mfrac>
                <mo>=</mo>
                <mfrac>
                    <mi>c</mi>
                    <mi>d</mi>
                </mfrac>
            </mrow>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        lines = result.split('\n')
        # The equals signs and fraction bars should be on the same line
        # Find the line with fraction bars
        for i, line in enumerate(lines):
            if '─' in line:
                # This should be the baseline with = signs too
                assert '=' in line
                break
    
    def test_nested_fractions(self):
        """Test fraction within a fraction."""
        parser = MathMLParser()
        mathml = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mfrac>
                <mfrac>
                    <mi>a</mi>
                    <mi>b</mi>
                </mfrac>
                <mi>c</mi>
            </mfrac>
        </math>
        '''
        box = parser.parse(mathml)
        result = box.render()
        lines = result.split('\n')
        # Should have multiple fraction bars at different levels
        fraction_bar_count = sum(1 for line in lines if '─' in line)
        assert fraction_bar_count >= 2


class TestMathMLToAscii:
    """Test the main conversion function."""
    
    def test_extract_mathml_from_html(self):
        html = '''
        <p>Here is a fraction: 
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mfrac><mi>x</mi><mi>y</mi></mfrac>
        </math>
        </p>
        '''
        result = mathml_to_ascii(html)
        assert 'x' in result
        assert '─' in result
        assert 'y' in result
    
    def test_multiple_math_elements(self):
        html = '''
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mi>a</mi>
        </math>
        and
        <math xmlns="http://www.w3.org/1998/Math/MathML">
            <mi>b</mi>
        </math>
        '''
        result = mathml_to_ascii(html)
        # Currently only processes first element
        assert 'a' in result
    
    def test_no_math_elements(self):
        html = '<p>Just plain text</p>'
        result = mathml_to_ascii(html)
        # When no math elements found, it returns the original HTML
        assert result == html
    
    def test_real_world_example(self):
        """Test with the actual complex expression provided."""
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
        result = mathml_to_ascii(html)
        print("\nReal world example output:")
        print(result)
        print()
        
        # Check that key components are present and properly arranged
        lines = result.split('\n')
        
        # Should have multiple lines due to subscripts and fractions
        assert len(lines) > 1
        
        # Check for key mathematical symbols
        assert 'E' in result
        assert 'P(x)' in result
        assert 'Q(x)' in result
        assert '∑' in result
        assert '=' in result
        assert '[' in result and ']' in result
        
        # Check that fraction bars exist
        assert '─' in result


if __name__ == "__main__":
    # Run tests without pytest
    import sys
    
    test_classes = [TestMathBox, TestMathMLParser, TestMathMLToAscii]
    
    total_tests = 0
    passed_tests = 0
    
    for test_class in test_classes:
        print(f"\nRunning {test_class.__name__}...")
        test_instance = test_class()
        
        for method_name in dir(test_instance):
            if method_name.startswith('test_'):
                total_tests += 1
                print(f"  {method_name}...", end=" ")
                try:
                    method = getattr(test_instance, method_name)
                    method()
                    print("✓")
                    passed_tests += 1
                except AssertionError as e:
                    print(f"✗ - {e}")
                except Exception as e:
                    print(f"ERROR - {e}")
    
    print(f"\n{passed_tests}/{total_tests} tests passed")
    
    if passed_tests < total_tests:
        sys.exit(1)