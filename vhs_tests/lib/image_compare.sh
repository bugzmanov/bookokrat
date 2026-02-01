#!/usr/bin/env bash
#
# image_compare.sh - Screenshot comparison utilities
#
# Compares actual screenshots against golden snapshots.

# Comparison result structure (stored as associative arrays)
# Results are written to a temp file for cross-function communication

# Compare two images
# Returns: 0 = match, 1 = mismatch, 2 = missing golden
# Outputs comparison details to stdout as KEY=VALUE lines
compare_images() {
    local golden="$1"
    local actual="$2"
    local name="$3"

    echo "NAME=$name"
    echo "GOLDEN=$golden"
    echo "ACTUAL=$actual"

    # Check if golden exists
    if [ ! -f "$golden" ]; then
        echo "STATUS=missing"
        echo "MESSAGE=Golden snapshot not found"
        return 2
    fi

    # Check if actual exists
    if [ ! -f "$actual" ]; then
        echo "STATUS=error"
        echo "MESSAGE=Actual screenshot not found"
        return 1
    fi

    # Get dimensions
    local golden_width=$(sips -g pixelWidth "$golden" 2>/dev/null | grep pixelWidth | awk '{print $2}')
    local golden_height=$(sips -g pixelHeight "$golden" 2>/dev/null | grep pixelHeight | awk '{print $2}')
    local actual_width=$(sips -g pixelWidth "$actual" 2>/dev/null | grep pixelWidth | awk '{print $2}')
    local actual_height=$(sips -g pixelHeight "$actual" 2>/dev/null | grep pixelHeight | awk '{print $2}')

    echo "GOLDEN_DIMS=${golden_width}x${golden_height}"
    echo "ACTUAL_DIMS=${actual_width}x${actual_height}"

    # Dimension check
    if [[ "$golden_width" != "$actual_width" ]] || [[ "$golden_height" != "$actual_height" ]]; then
        echo "STATUS=mismatch"
        echo "MESSAGE=Dimension mismatch: ${golden_width}x${golden_height} vs ${actual_width}x${actual_height}"
        return 1
    fi

    # File sizes (for reporting)
    local golden_size=$(stat -f%z "$golden")
    local actual_size=$(stat -f%z "$actual")

    echo "GOLDEN_SIZE=$golden_size"
    echo "ACTUAL_SIZE=$actual_size"

    # SSIM comparison (pass if SSIM >= 0.98), fallback to pixel diff
    local ssim_value=""
    local total_pixels=$((golden_width * golden_height))
    local diff_pct=""

    if command -v compare >/dev/null 2>&1; then
        ssim_value=$(compare -metric SSIM "$golden" "$actual" null: 2>&1 || true)
        ssim_value=$(echo "$ssim_value" | awk '{print $1}' | sed -E 's/[^0-9.].*$//')
        if ! [[ "$ssim_value" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
            ssim_value=""
        fi
    fi

    if [ -n "$ssim_value" ]; then
        echo "SSIM=$ssim_value"
        if (( $(echo "$ssim_value >= 0.98" | bc -l) )); then
            echo "STATUS=match"
            echo "MESSAGE=SSIM ${ssim_value}"
            return 0
        fi
        echo "STATUS=mismatch"
        echo "MESSAGE=SSIM ${ssim_value} (< 0.98)"
        return 1
    fi

    if command -v compare >/dev/null 2>&1; then
        local diff_pixels
        diff_pixels=$(compare -metric AE "$golden" "$actual" null: 2>&1 || true)
        if [[ "$diff_pixels" =~ ^[0-9]+$ ]]; then
            diff_pct=$(echo "scale=6; $diff_pixels * 100 / $total_pixels" | bc -l)
        fi
    fi

    if [ -z "$diff_pct" ]; then
        diff_pct=$(swift - "$golden" "$actual" 2>/dev/null <<'SWIFT'
import Foundation
import CoreGraphics
import ImageIO

func loadImage(_ path: String) -> CGImage? {
    let url = URL(fileURLWithPath: path)
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
    return CGImageSourceCreateImageAtIndex(source, 0, nil)
}

func rasterize(_ image: CGImage) -> [UInt8]? {
    let width = image.width
    let height = image.height
    let bytesPerPixel = 4
    let bytesPerRow = width * bytesPerPixel
    var data = [UInt8](repeating: 0, count: height * bytesPerRow)
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
    guard let ctx = CGContext(
        data: &data,
        width: width,
        height: height,
        bitsPerComponent: 8,
        bytesPerRow: bytesPerRow,
        space: colorSpace,
        bitmapInfo: bitmapInfo
    ) else { return nil }
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    return data
}

func downsampledLuma(_ image: CGImage, targetWidth: Int) -> ([UInt8], Int, Int)? {
    let width = image.width
    let height = image.height
    let scale = Double(targetWidth) / Double(max(width, 1))
    let targetHeight = max(Int(Double(height) * scale), 1)
    let bytesPerPixel = 4
    let bytesPerRow = targetWidth * bytesPerPixel
    var data = [UInt8](repeating: 0, count: targetHeight * bytesPerRow)
    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
    guard let ctx = CGContext(
        data: &data,
        width: targetWidth,
        height: targetHeight,
        bitsPerComponent: 8,
        bytesPerRow: bytesPerRow,
        space: colorSpace,
        bitmapInfo: bitmapInfo
    ) else { return nil }
    ctx.interpolationQuality = .low
    ctx.draw(image, in: CGRect(x: 0, y: 0, width: targetWidth, height: targetHeight))
    var luma = [UInt8](repeating: 0, count: targetWidth * targetHeight)
    for y in 0..<targetHeight {
        let rowBase = y * bytesPerRow
        let lumaBase = y * targetWidth
        for x in 0..<targetWidth {
            let idx = rowBase + x * bytesPerPixel
            let r = Double(data[idx])
            let g = Double(data[idx + 1])
            let b = Double(data[idx + 2])
            let yVal = 0.2126 * r + 0.7152 * g + 0.0722 * b
            luma[lumaBase + x] = UInt8(min(max(Int(yVal.rounded()), 0), 255))
        }
    }
    return (luma, targetWidth, targetHeight)
}

let args = CommandLine.arguments
guard args.count >= 3 else { exit(1) }
let goldenPath = args[1]
let actualPath = args[2]

guard let goldenImage = loadImage(goldenPath),
      let actualImage = loadImage(actualPath) else { exit(1) }

guard goldenImage.width == actualImage.width,
      goldenImage.height == actualImage.height else { exit(1) }

let targetWidth = 320
guard let (goldenLuma, gw, gh) = downsampledLuma(goldenImage, targetWidth: targetWidth),
      let (actualLuma, aw, ah) = downsampledLuma(actualImage, targetWidth: targetWidth),
      gw == aw, gh == ah else { exit(1) }

let totalPixels = gw * gh
var totalDiff: Double = 0
for i in 0..<totalPixels {
    totalDiff += abs(Double(goldenLuma[i]) - Double(actualLuma[i]))
}
let meanDiff = totalDiff / Double(totalPixels)
let diffPct = (meanDiff / 255.0) * 100.0
print(String(format: "%.6f", diffPct))
SWIFT
)
    fi

    if [ -z "$diff_pct" ]; then
        echo "STATUS=mismatch"
        echo "MESSAGE=Diff calculation failed"
        return 1
    fi

    echo "DIFF_PCT=$diff_pct"

    if (( $(echo "$diff_pct < 5.0" | bc -l) )); then
        echo "STATUS=match"
        echo "MESSAGE=Diff ${diff_pct}%"
        return 0
    fi

    echo "STATUS=mismatch"
    echo "MESSAGE=Diff ${diff_pct}% (>= 5%)"
    return 1

}

# Compare all screenshots from a tape run
# Usage: compare_all_screenshots GOLDEN_DIR ACTUAL_DIR SCREENSHOT_NAMES...
# Outputs results to stdout, one comparison block per screenshot
compare_all_screenshots() {
    local golden_dir="$1"
    local actual_dir="$2"
    shift 2
    local screenshots=("$@")

    local passed=0
    local failed=0
    local missing=0

    echo "TAPE_RESULTS_START"

    for name in "${screenshots[@]}"; do
        echo "---"
        local golden="$golden_dir/$name.png"
        local actual="$actual_dir/$name.png"

        compare_images "$golden" "$actual" "$name"
        local result=$?

        case $result in
            0) passed=$((passed + 1)) ;;
            1) failed=$((failed + 1)) ;;
            2) missing=$((missing + 1)) ;;
        esac
    done

    echo "---"
    echo "TAPE_RESULTS_END"
    echo "SUMMARY_PASSED=$passed"
    echo "SUMMARY_FAILED=$failed"
    echo "SUMMARY_MISSING=$missing"
    echo "SUMMARY_TOTAL=${#screenshots[@]}"
}

# Update golden snapshots
# Usage: update_golden ACTUAL_DIR GOLDEN_DIR SCREENSHOT_NAMES...
update_golden_snapshots() {
    local actual_dir="$1"
    local golden_dir="$2"
    shift 2
    local screenshots=("$@")

    mkdir -p "$golden_dir"

    local updated=0
    for name in "${screenshots[@]}"; do
        local actual="$actual_dir/$name.png"
        local golden="$golden_dir/$name.png"

        if [ -f "$actual" ]; then
            cp "$actual" "$golden"
            echo "Updated: $name.png"
            updated=$((updated + 1))
        else
            echo "Warning: $actual not found, skipping"
        fi
    done

    echo "Updated $updated golden snapshots"
}

# Convert image to base64 data URI
image_to_base64() {
    local image_path="$1"
    if [ -f "$image_path" ]; then
        echo "data:image/png;base64,$(base64 < "$image_path")"
    else
        echo ""
    fi
}

# Generate a diff image highlighting differences in red
# Usage: generate_diff_image GOLDEN ACTUAL OUTPUT_DIFF
# Returns 0 if diff was generated, 1 if images match or error
generate_diff_image() {
    local golden="$1"
    local actual="$2"
    local diff_output="$3"

    if [ ! -f "$golden" ] || [ ! -f "$actual" ]; then
        return 1
    fi

    # Use ImageMagick if available (best quality diff)
    if command -v magick &>/dev/null; then
        # Generate diff with red highlighting for changed pixels
        magick compare -highlight-color red -lowlight-color 'rgba(0,0,0,0)' \
            -compose src "$golden" "$actual" "$diff_output" 2>/dev/null
        return $?
    elif command -v compare &>/dev/null; then
        # Older ImageMagick
        compare -highlight-color red -lowlight-color 'rgba(0,0,0,0)' \
            -compose src "$golden" "$actual" "$diff_output" 2>/dev/null
        return $?
    fi

    # Fallback: Use sips + CoreImage via Swift for basic diff
    swift -e "
import Cocoa
import CoreImage

let goldenURL = URL(fileURLWithPath: \"$golden\")
let actualURL = URL(fileURLWithPath: \"$actual\")
let outputURL = URL(fileURLWithPath: \"$diff_output\")

guard let goldenImage = CIImage(contentsOf: goldenURL),
      let actualImage = CIImage(contentsOf: actualURL) else { exit(1) }

// Create difference filter
let diffFilter = CIFilter(name: \"CIDifferenceBlendMode\")!
diffFilter.setValue(goldenImage, forKey: kCIInputImageKey)
diffFilter.setValue(actualImage, forKey: kCIInputBackgroundImageKey)

guard let diffOutput = diffFilter.outputImage else { exit(1) }

// Colorize the diff (make non-black pixels red)
let colorMatrix = CIFilter(name: \"CIColorMatrix\")!
colorMatrix.setValue(diffOutput, forKey: kCIInputImageKey)
colorMatrix.setValue(CIVector(x: 3, y: 0, z: 0, w: 0), forKey: \"inputRVector\")  // Boost red
colorMatrix.setValue(CIVector(x: 0, y: 0.3, z: 0, w: 0), forKey: \"inputGVector\")
colorMatrix.setValue(CIVector(x: 0, y: 0, z: 0.3, w: 0), forKey: \"inputBVector\")
colorMatrix.setValue(CIVector(x: 0, y: 0, z: 0, w: 1), forKey: \"inputAVector\")

guard let colorizedDiff = colorMatrix.outputImage else { exit(1) }

// Composite diff over actual image
let composite = CIFilter(name: \"CISourceOverCompositing\")!
composite.setValue(colorizedDiff, forKey: kCIInputImageKey)
composite.setValue(actualImage, forKey: kCIInputBackgroundImageKey)

guard let finalImage = composite.outputImage else { exit(1) }

let context = CIContext()
let cgImage = context.createCGImage(finalImage, from: finalImage.extent)!
let nsImage = NSBitmapImageRep(cgImage: cgImage)
let pngData = nsImage.representation(using: .png, properties: [:])!
try! pngData.write(to: outputURL)
" 2>/dev/null

    return $?
}
