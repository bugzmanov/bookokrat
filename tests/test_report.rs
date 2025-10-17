use once_cell::sync::Lazy;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

pub struct TestFailure {
    pub test_name: String,
    pub expected: String,
    pub actual: String,
    pub line_stats: LineStats,
    #[allow(dead_code)]
    pub snapshot_path: String,
}

pub struct LineStats {
    pub expected_lines: usize,
    pub actual_lines: usize,
    pub diff_count: usize,
    pub first_diff_line: Option<usize>,
}

pub struct TestReport {
    failures: Vec<TestFailure>,
    browser_opened: bool,
}

static TEST_REPORT: Lazy<Mutex<TestReport>> = Lazy::new(|| {
    Mutex::new(TestReport {
        failures: Vec::new(),
        browser_opened: false,
    })
});

impl TestReport {
    pub fn add_failure(failure: TestFailure) {
        if let Ok(mut report) = TEST_REPORT.lock() {
            report.failures.push(failure);
        }
    }

    #[allow(dead_code)]
    pub fn generate_and_open_if_failures() {
        if let Ok(mut report) = TEST_REPORT.lock() {
            if !report.failures.is_empty() && !report.browser_opened {
                report.browser_opened = true;
                let html = report.generate_html();
                if let Err(e) = report.save_and_open(html) {
                    eprintln!("Failed to generate test report: {e}");
                }
            }
        }
    }

    /// Force generate and open the report if there are failures, regardless of whether it was already opened
    pub fn force_generate_and_open_if_failures() {
        if let Ok(mut report) = TEST_REPORT.lock() {
            if !report.failures.is_empty() {
                report.browser_opened = true;
                let html = report.generate_html();
                if let Err(e) = report.save_and_open(html) {
                    eprintln!("Failed to generate test report: {e}");
                }
            }
        }
    }

    fn generate_html(&self) -> String {
        let test_sections: String = self
            .failures
            .iter()
            .map(|failure| {
                format!(
                    r#"
        <div class="test-section" data-test="{}">
            <div class="test-header">
                <h2>❌ {}</h2>
                <div class="test-stats">
                    <span>📊 Lines: {} → {}</span>
                    <span>⚠️ Differences: {}</span>
                    {}
                </div>
            </div>
            
            <div class="side-by-side">
                <div class="svg-container expected">
                    <h3>✅ Expected</h3>
                    <div class="svg-wrapper">
                        {}
                    </div>
                </div>
                
                <div class="svg-container actual">
                    <h3>❌ Actual</h3>
                    <div class="svg-wrapper">
                        {}
                    </div>
                </div>
            </div>
            
            <div class="test-actions">
                <button class="button update-btn" data-test="{}" onclick="copyCommand('{}')">
                    📋 Copy Update Command
                </button>
            </div>
        </div>"#,
                    failure.test_name,
                    failure.test_name,
                    failure.line_stats.expected_lines,
                    failure.line_stats.actual_lines,
                    failure.line_stats.diff_count,
                    failure
                        .line_stats
                        .first_diff_line
                        .map(|line| format!("<span>📍 First diff: line {line}</span>"))
                        .unwrap_or_default(),
                    failure.expected,
                    failure.actual,
                    failure.test_name,
                    failure.test_name
                )
            })
            .collect();

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>SVG Snapshot Test Report</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 0;
            background: #f5f5f5;
        }}
        .header {{
            background: #d32f2f;
            color: white;
            padding: 30px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
            position: sticky;
            top: 0;
            z-index: 100;
        }}
        .header h1 {{
            margin: 0;
            font-size: 28px;
        }}
        .header p {{
            margin: 10px 0 0 0;
            opacity: 0.9;
            font-size: 16px;
        }}
        .container {{
            max-width: 1600px;
            margin: 0 auto;
            padding: 20px;
        }}
        .summary {{
            background: white;
            border-radius: 8px;
            padding: 20px;
            margin: 20px 0;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .summary h2 {{
            margin: 0 0 15px 0;
            color: #333;
            font-size: 20px;
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
        .failures-list {{
            background: white;
            border-radius: 8px;
            padding: 20px;
            margin: 20px 0;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .failures-list h2 {{
            margin: 0 0 15px 0;
            color: #333;
            font-size: 20px;
        }}
        .failures-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 10px;
            margin-top: 15px;
        }}
        .failure-link {{
            display: block;
            padding: 12px 16px;
            background: #f8f9fa;
            border: 1px solid #e9ecef;
            border-radius: 6px;
            color: #d32f2f;
            text-decoration: none;
            font-size: 14px;
            font-family: monospace;
            transition: all 0.2s;
        }}
        .failure-link:hover {{
            background: #e3f2fd;
            border-color: #bbdefb;
            text-decoration: none;
        }}
        .test-actions {{
            margin-top: 20px;
            text-align: center;
            padding-top: 20px;
            border-top: 1px solid #e0e0e0;
        }}
        .button {{
            padding: 10px 20px;
            margin: 0 10px;
            border: none;
            border-radius: 4px;
            font-size: 14px;
            cursor: pointer;
            transition: all 0.2s;
        }}
        .update-btn {{
            background: #2196f3;
            color: white;
        }}
        .update-btn:hover {{
            background: #1976d2;
        }}
        .update-all-btn {{
            background: #ff9800;
            color: white;
        }}
        .update-all-btn:hover {{
            background: #f57c00;
        }}
        .success {{
            background: #d4edda !important;
            color: #155724 !important;
        }}
        .error {{
            background: #f8d7da !important;
            color: #721c24 !important;
        }}
        .copy-notification {{
            position: fixed;
            top: 20px;
            left: 50%;
            transform: translateX(-50%);
            padding: 15px 25px;
            border-radius: 4px;
            font-size: 14px;
            background: #4caf50;
            color: white;
            box-shadow: 0 2px 8px rgba(0,0,0,0.2);
            opacity: 0;
            transition: opacity 0.3s;
            z-index: 1000;
        }}
        .copy-notification.show {{
            opacity: 1;
        }}
        .test-section {{
            background: white;
            border-radius: 8px;
            padding: 20px;
            margin: 20px 0;
            box-shadow: 0 2px 4px rgba(0,0,0,0.05);
        }}
        .test-header {{
            border-bottom: 1px solid #e0e0e0;
            padding-bottom: 15px;
            margin-bottom: 20px;
        }}
        .test-header h2 {{
            margin: 0 0 10px 0;
            color: #d32f2f;
            font-size: 20px;
        }}
        .test-stats {{
            display: flex;
            gap: 20px;
            color: #666;
            font-size: 14px;
        }}
        .test-stats span {{
            display: inline-block;
        }}
        .side-by-side {{
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 20px;
        }}
        .svg-container {{
            border: 1px solid #ddd;
            border-radius: 4px;
            overflow: hidden;
        }}
        .svg-container h3 {{
            margin: 0;
            padding: 10px 15px;
            background: #f5f5f5;
            border-bottom: 1px solid #ddd;
            font-size: 16px;
            font-weight: 600;
        }}
        .expected h3 {{
            color: #2e7d32;
            background: #e8f5e9;
        }}
        .actual h3 {{
            color: #d32f2f;
            background: #ffebee;
        }}
        .svg-wrapper {{
            overflow: auto;
            max-height: 500px;
            background: #fafafa;
            padding: 10px;
        }}
        .svg-wrapper svg {{
            display: block;
            margin: 0 auto;
        }}
    </style>
</head>
<body>
    <div class="header">
        <h1>🚨 SVG Snapshot Test Report</h1>
        <p>{} test{} failed</p>
    </div>
    
    <div class="container">
        <div class="summary">
            <h2>Summary</h2>
            <p>The following SVG snapshot tests have failed. Review the visual differences below.</p>
        </div>
        
        <div class="info-box">
            <strong>💡 To update all snapshots:</strong> <code>SNAPSHOTS=overwrite cargo test</code><br>
            <strong>💡 To update specific snapshot:</strong> <code>SNAPSHOTS=overwrite cargo test [test_name]</code><br>
            <br>
            <button class="button update-all-btn" onclick="copyAllCommand()">
                📋 Copy Update All Command
            </button>
        </div>
        
        <div class="failures-list">
            <h2>Failed Tests</h2>
            <p>Click on any test name to jump to its detailed comparison:</p>
            <div class="failures-grid">
                {}
            </div>
        </div>
        
        {}
    </div>
    
    <div class="copy-notification" id="copyNotification"></div>
    
    <script>
        // Show notification
        function showNotification(message) {{
            const notification = document.getElementById('copyNotification');
            notification.textContent = message;
            notification.classList.add('show');
            setTimeout(() => {{
                notification.classList.remove('show');
            }}, 2000);
        }}
        
        // Copy individual test command
        async function copyCommand(testName) {{
            const command = `SNAPSHOTS=overwrite cargo test ${{testName}}`;
            
            try {{
                await navigator.clipboard.writeText(command);
                showNotification(`✅ Copied: ${{command}}`);
                
                // Update button temporarily
                const button = document.querySelector(`[data-test="${{testName}}"]`);
                const originalText = button.textContent;
                button.textContent = '✅ Copied!';
                button.classList.add('success');
                
                setTimeout(() => {{
                    button.textContent = originalText;
                    button.classList.remove('success');
                }}, 1500);
            }} catch (err) {{
                // Fallback for browsers that don't support clipboard API
                const textArea = document.createElement('textarea');
                textArea.value = command;
                textArea.style.position = 'fixed';
                textArea.style.left = '-999999px';
                document.body.appendChild(textArea);
                textArea.select();
                try {{
                    document.execCommand('copy');
                    showNotification(`✅ Copied: ${{command}}`);
                }} catch (err) {{
                    showNotification('❌ Failed to copy command');
                }}
                document.body.removeChild(textArea);
            }}
        }}
        
        // Copy all tests command
        async function copyAllCommand() {{
            const command = 'SNAPSHOTS=overwrite cargo test --test svg_snapshots';
            
            try {{
                await navigator.clipboard.writeText(command);
                showNotification(`✅ Copied: ${{command}}`);
                
                // Update button temporarily
                const button = event.target;
                const originalText = button.textContent;
                button.textContent = '✅ Copied!';
                button.classList.add('success');
                
                setTimeout(() => {{
                    button.textContent = originalText;
                    button.classList.remove('success');
                }}, 1500);
            }} catch (err) {{
                // Fallback for browsers that don't support clipboard API
                const textArea = document.createElement('textarea');
                textArea.value = command;
                textArea.style.position = 'fixed';
                textArea.style.left = '-999999px';
                document.body.appendChild(textArea);
                textArea.select();
                try {{
                    document.execCommand('copy');
                    showNotification(`✅ Copied: ${{command}}`);
                }} catch (err) {{
                    showNotification('❌ Failed to copy command');
                }}
                document.body.removeChild(textArea);
            }}
        }}
        
        // Sync scrolling between SVG containers within each test
        document.querySelectorAll('.test-section').forEach(section => {{
            const svgWrappers = section.querySelectorAll('.svg-wrapper');
            svgWrappers.forEach((wrapper, index) => {{
                wrapper.addEventListener('scroll', () => {{
                    const otherWrapper = svgWrappers[1 - index];
                    if (otherWrapper) {{
                        otherWrapper.scrollTop = wrapper.scrollTop;
                        otherWrapper.scrollLeft = wrapper.scrollLeft;
                    }}
                }});
            }});
        }});
    </script>
</body>
</html>"#,
            self.failures.len(),
            if self.failures.len() == 1 { "" } else { "s" },
            self.failures.iter().map(|f| {
                format!(r##"<a href="#" class="failure-link" onclick="document.querySelector('[data-test=\"{}\"]').scrollIntoView({{behavior: 'smooth'}}); return false;">{}</a>"##, 
                    f.test_name, f.test_name)
            }).collect::<Vec<_>>().join("\n                "),
            test_sections
        )
    }

    fn save_and_open(&self, html: String) -> std::io::Result<()> {
        self.save_and_open_with_flag(html, true)
    }

    fn save_and_open_with_flag(&self, html: String, should_open: bool) -> std::io::Result<()> {
        // Create output directory
        let output_dir = Path::new("target/test-reports");
        fs::create_dir_all(output_dir)?;

        // Generate filename
        let output_path = output_dir.join("svg_snapshot_report.html");

        // Write HTML
        fs::write(&output_path, html)?;

        // Check if we should open the browser
        if should_open && std::env::var("OPEN_REPORT").is_ok() {
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
                    "\n⚠️  Failed to open browser: {}. Report saved to: {}",
                    e,
                    output_path.display()
                );
            } else {
                eprintln!(
                    "\n📊 Full snapshot report opened in browser: {}",
                    output_path.display()
                );
            }
        } else {
            eprintln!("\n📊 Snapshot report saved to: {}", output_path.display());
            if !should_open {
                eprintln!("   💡 Run with OPEN_REPORT=1 to automatically open in browser");
            }
        }

        Ok(())
    }
}

// Initialize the report system and set up finalization
pub fn init_test_report() {
    // Set up an atexit handler to generate the report when the process finishes
    // This ensures we only open the browser once at the end, after all tests complete
    extern "C" fn generate_final_report() {
        TestReport::force_generate_and_open_if_failures();
    }

    unsafe {
        libc::atexit(generate_final_report);
    }

    // Register a panic hook that just collects failures but doesn't generate/open anything
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Call the original panic hook
        original_hook(panic_info);

        // Don't generate reports here - let atexit handle everything
    }));
}
