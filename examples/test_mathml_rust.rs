//! Test the Rust MathML renderer with the same examples as Python

use bookrat::mathml_renderer::mathml_to_ascii;

fn main() {
    println!("Testing Rust MathML Renderer");
    println!("{}", "=".repeat(60));

    // The complex equation from the user's example
    let html = r#"
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

    println!("Complex Equation Rendering:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(html, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    // Also test a simpler fraction alignment
    let simple = r#"
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

    println!("\nSimple Fraction Alignment Test:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(simple, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    let super_complex = r#"
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

    println!("\nSuper complex test:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(super_complex, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    let duper = r#"
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

    println!("\nDuper complex test:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(duper, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    // Square root examples
    let sqrt_simple = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <msqrt>
        <mrow><mi>x</mi><mo>+</mo><mn>1</mn></mrow>
    </msqrt>
</math>
"#;

    println!("\nSimple Square Root Test:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(sqrt_simple, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    // Complex square root with fraction
    let sqrt_complex = r#"
<math xmlns="http://www.w3.org/1998/Math/MathML">
    <msqrt>
        <mfrac>
            <mrow><msup><mi>x</mi><mn>2</mn></msup><mo>+</mo><msub><mi>b</mi><mi>c</mi></msub></mrow>
            <mrow><mi>sin</mi><mo>(</mo><mi>x</mi><mo>)</mo><msup><mi>cos</mi><mrow><mo>(</mo><mi>y</mi><mo>)</mo></mrow></msup><mo>+</mo><msup><mi>e</mi><mrow><mi>z</mi><mo>⋅</mo><mn>5</mn></mrow></msup></mrow>
        </mfrac>
    </msqrt>
</math>
"#;

    println!("\nComplex Square Root with Fraction:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(sqrt_complex, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    // ----
    let complex_multiline = r#"
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

    println!("\nComplex Multiline:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(complex_multiline, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));

    // ----
    let complex_integral = r#"
        <math alttext="\displaystyle-\int_{\mathbb{R}^{D}}p(\bm{\xi})\log\frac{p(\bm{\xi})}{q(\bm{\xi})}\mathrm{d}\bm{\xi}=\int_{\mathbb{R}^{D}}p(\bm{\xi})\log\frac{q(\bm{\xi})}{p(\bm{\xi})}\mathrm{d}\bm{\xi}" class="ltx_Math" display="inline" id="S1.Ex1.m3"><semantics><mrow><mrow><mo>−</mo><mrow><mstyle displaystyle="true"><msub><mo>∫</mo><msup><mi>ℝ</mi><mi>D</mi></msup></msub></mstyle><mrow><mi>p</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow><mo lspace="0.167em" rspace="0em">​</mo><mrow><mi>log</mi><mo lspace="0.167em">⁡</mo><mrow><mstyle displaystyle="true"><mfrac><mrow><mi>p</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow></mrow><mrow><mi>q</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow></mrow></mfrac></mstyle><mo lspace="0em" rspace="0em">​</mo><mi mathvariant="normal">d</mi><mo lspace="0em" rspace="0em">​</mo><mi>𝝃</mi></mrow></mrow></mrow></mrow></mrow><mo>=</mo><mrow><mstyle displaystyle="true"><msub><mo>∫</mo><msup><mi>ℝ</mi><mi>D</mi></msup></msub></mstyle><mrow><mi>p</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow><mo lspace="0.167em" rspace="0em">​</mo><mrow><mi>log</mi><mo lspace="0.167em">⁡</mo><mrow><mstyle displaystyle="true"><mfrac><mrow><mi>q</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow></mrow><mrow><mi>p</mi><mo lspace="0em" rspace="0em">​</mo><mrow><mo stretchy="false">(</mo><mi>𝝃</mi><mo stretchy="false">)</mo></mrow></mrow></mfrac></mstyle><mo lspace="0em" rspace="0em">​</mo><mi mathvariant="normal">d</mi><mo lspace="0em" rspace="0em">​</mo><mi>𝝃</mi></mrow></mrow></mrow></mrow></mrow><annotation encoding="application/x-tex">\displaystyle-\int_{\mathbb{R}^{D}}p(\bm{\xi})\log\frac{p(\bm{\xi})}{q(\bm{\xi})}\mathrm{d}\bm{\xi}=\int_{\mathbb{R}^{D}}p(\bm{\xi})\log\frac{q(\bm{\xi})}{p(\bm{\xi})}\mathrm{d}\bm{\xi}</annotation><annotation encoding="application/x-llamapun">- ∫ start_POSTSUBSCRIPT blackboard_R start_POSTSUPERSCRIPT italic_D end_POSTSUPERSCRIPT end_POSTSUBSCRIPT italic_p ( bold_italic_ξ ) roman_log divide start_ARG italic_p ( bold_italic_ξ ) end_ARG start_ARG italic_q ( bold_italic_ξ ) end_ARG roman_d bold_italic_ξ = ∫ start_POSTSUBSCRIPT blackboard_R start_POSTSUPERSCRIPT italic_D end_POSTSUPERSCRIPT end_POSTSUBSCRIPT italic_p ( bold_italic_ξ ) roman_log divide start_ARG italic_q ( bold_italic_ξ ) end_ARG start_ARG italic_p ( bold_italic_ξ ) end_ARG roman_d bold_italic_ξ</annotation></semantics></math>
        "#;

    println!("\nComplex integral:");
    println!("{}", "=".repeat(60));
    match mathml_to_ascii(complex_integral, true) {
        Ok(result) => println!("{}", result),
        Err(e) => println!("Error: {}", e),
    }
    println!("{}", "=".repeat(60));
}
