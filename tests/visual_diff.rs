#![allow(dead_code)]

use std::fs;
use std::path::Path;
use std::process::Command;

pub struct VisualDiffViewer {
    test_name: String,
    expected: String,
    actual: String,
}

impl VisualDiffViewer {
    pub fn new(test_name: &str, expected: String, actual: String) -> Self {
        Self {
            test_name: test_name.to_string(),
            expected,
            actual,
        }
    }

    pub fn generate_html_report(&self) -> String {
        let diff_html = self.generate_diff_html();

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SVG Snapshot Test Failure: {}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background: #f5f5f5;
        }}
        .header {{
            background: #d32f2f;
            color: white;
            padding: 20px;
            margin: -20px -20px 20px -20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        .header h1 {{
            margin: 0;
            font-size: 24px;
        }}
        .header p {{
            margin: 5px 0 0 0;
            opacity: 0.9;
        }}
        .container {{
            max-width: 1400px;
            margin: 0 auto;
        }}
        .side-by-side {{
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 20px;
            margin-bottom: 30px;
        }}
        .svg-container {{
            background: white;
            border: 1px solid #ddd;
            border-radius: 8px;
            padding: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .svg-container h2 {{
            margin: 0 0 15px 0;
            color: #333;
            font-size: 18px;
            font-weight: 600;
        }}
        .expected h2 {{
            color: #2e7d32;
        }}
        .actual h2 {{
            color: #d32f2f;
        }}
        .svg-wrapper {{
            border: 1px solid #e0e0e0;
            border-radius: 4px;
            overflow: auto;
            max-height: 600px;
            background: #fafafa;
        }}
        .svg-wrapper svg {{
            display: block;
            margin: 0 auto;
        }}
        .diff-section {{
            background: white;
            border: 1px solid #ddd;
            border-radius: 8px;
            padding: 20px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .diff-section h2 {{
            margin: 0 0 15px 0;
            color: #333;
            font-size: 18px;
            font-weight: 600;
        }}
        .diff-content {{
            font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
            font-size: 12px;
            line-height: 1.5;
            overflow-x: auto;
            background: #f8f8f8;
            border: 1px solid #e0e0e0;
            border-radius: 4px;
            padding: 15px;
            max-height: 400px;
            overflow-y: auto;
        }}
        .diff-line {{
            margin: 0;
            padding: 2px 5px;
            white-space: pre-wrap;
            word-break: break-all;
        }}
        .diff-added {{
            background-color: #e8f5e9;
            color: #2e7d32;
        }}
        .diff-removed {{
            background-color: #ffebee;
            color: #c62828;
        }}
        .diff-context {{
            color: #666;
        }}
        .line-number {{
            display: inline-block;
            width: 50px;
            color: #999;
            text-align: right;
            margin-right: 10px;
            user-select: none;
        }}
        .actions {{
            margin: 20px 0;
            text-align: center;
        }}
        .button {{
            display: inline-block;
            padding: 10px 20px;
            margin: 0 10px;
            background: #1976d2;
            color: white;
            text-decoration: none;
            border-radius: 4px;
            font-size: 14px;
            cursor: pointer;
            border: none;
            transition: background 0.2s;
        }}
        .button:hover {{
            background: #1565c0;
        }}
        .button.secondary {{
            background: #757575;
        }}
        .button.secondary:hover {{
            background: #616161;
        }}
        .info-box {{
            background: #e3f2fd;
            border: 1px solid #bbdefb;
            border-radius: 4px;
            padding: 15px;
            margin: 20px 0;
            color: #1565c0;
        }}
        .info-box code {{
            background: #bbdefb;
            padding: 2px 6px;
            border-radius: 3px;
            font-family: 'Consolas', 'Monaco', 'Courier New', monospace;
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>🚨 SVG Snapshot Test Failed</h1>
        <p>Test: {}</p>
    </div>
    
    <div class="container">
        <div class="info-box">
            <strong>Tip:</strong> To update the snapshot, run: <code>SNAPSHOTS=overwrite cargo test {}</code>
        </div>
        
        <div class="side-by-side">
            <div class="svg-container expected">
                <h2>✅ Expected</h2>
                <div class="svg-wrapper">
                    {}
                </div>
            </div>
            
            <div class="svg-container actual">
                <h2>❌ Actual</h2>
                <div class="svg-wrapper">
                    {}
                </div>
            </div>
        </div>
        
        <div class="diff-section">
            <h2>📊 Diff Details</h2>
            <div class="diff-content">
                {}
            </div>
        </div>
        
        <div class="actions">
            <button class="button" onclick="location.reload()">🔄 Refresh</button>
            <button class="button secondary" onclick="window.close()">✖️ Close</button>
        </div>
    </div>
    
    <script>
        // Sync scrolling between SVG containers
        const svgWrappers = document.querySelectorAll('.svg-wrapper');
        svgWrappers.forEach((wrapper, index) => {{
            wrapper.addEventListener('scroll', () => {{
                const otherWrapper = svgWrappers[1 - index];
                otherWrapper.scrollTop = wrapper.scrollTop;
                otherWrapper.scrollLeft = wrapper.scrollLeft;
            }});
        }});
    </script>
</body>
</html>"#,
            self.test_name, self.test_name, self.test_name, self.expected, self.actual, diff_html
        )
    }

    fn generate_diff_html(&self) -> String {
        let expected_lines: Vec<&str> = self.expected.lines().collect();
        let actual_lines: Vec<&str> = self.actual.lines().collect();

        let mut diff_html = String::new();

        // Simple line-by-line diff
        let max_lines = expected_lines.len().max(actual_lines.len());

        for i in 0..max_lines {
            let expected_line = expected_lines.get(i).copied().unwrap_or("");
            let actual_line = actual_lines.get(i).copied().unwrap_or("");

            if expected_line != actual_line {
                // Show context before (if available)
                if i > 0 && i < expected_lines.len() && i < actual_lines.len() {
                    let prev_line = expected_lines.get(i - 1).copied().unwrap_or("");
                    diff_html.push_str(&format!(
                        r#"<div class="diff-line diff-context"><span class="line-number">{}</span>{}</div>"#,
                        i,
                        html_escape(prev_line)
                    ));
                }

                // Show removed line
                if i < expected_lines.len() && expected_line != actual_line {
                    diff_html.push_str(&format!(
                        r#"<div class="diff-line diff-removed"><span class="line-number">{}</span>- {}</div>"#,
                        i + 1,
                        html_escape(expected_line)
                    ));
                }

                // Show added line
                if i < actual_lines.len() && expected_line != actual_line {
                    diff_html.push_str(&format!(
                        r#"<div class="diff-line diff-added"><span class="line-number">{}</span>+ {}</div>"#,
                        i + 1,
                        html_escape(actual_line)
                    ));
                }

                // Show context after (if available)
                if i + 1 < expected_lines.len() && i + 1 < actual_lines.len() {
                    let next_line = expected_lines.get(i + 1).copied().unwrap_or("");
                    if expected_lines.get(i + 1) == actual_lines.get(i + 1) {
                        diff_html.push_str(&format!(
                            r#"<div class="diff-line diff-context"><span class="line-number">{}</span>{}</div>"#,
                            i + 2,
                            html_escape(next_line)
                        ));
                    }
                }
            }
        }

        if diff_html.is_empty() {
            diff_html =
                r#"<div class="diff-line diff-context">No differences found</div>"#.to_string();
        }

        diff_html
    }

    pub fn save_and_open(&self) -> std::io::Result<()> {
        // Create output directory
        let output_dir = Path::new("target/test-reports");
        fs::create_dir_all(output_dir)?;

        // Generate filename
        let filename = format!("{}_diff.html", self.test_name);
        let output_path = output_dir.join(&filename);

        // Write HTML
        let html = self.generate_html_report();
        fs::write(&output_path, html)?;

        // Try to open in browser
        let open_result = if cfg!(target_os = "macos") {
            Command::new("open").arg(&output_path).spawn()
        } else if cfg!(target_os = "linux") {
            Command::new("xdg-open").arg(&output_path).spawn()
        } else if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", "start", output_path.to_str().unwrap()])
                .spawn()
        } else {
            return Ok(());
        };

        if let Err(e) = open_result {
            eprintln!(
                "Failed to open browser: {}. Report saved to: {}",
                e,
                output_path.display()
            );
        } else {
            eprintln!(
                "\n📊 Visual diff report opened in browser: {}",
                output_path.display()
            );
        }

        Ok(())
    }
}

fn html_escape(s: &str) -> String {
    s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}
