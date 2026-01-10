#!/usr/bin/env python3
"""
Script to fix incorrect word_range values in bookokrat comment files.
Removes word_range fields that are clearly character offsets instead of word indices.
"""

import yaml
import sys

def count_words(text):
    """Count words in text, matching the Rust implementation."""
    word_count = 0
    in_word = False

    for ch in text:
        if ch.isspace():
            in_word = False
        elif not in_word:
            word_count += 1
            in_word = True

    return word_count

def fix_comment(comment):
    """Fix a single comment entry."""
    # Only process paragraph comments with word_range
    if comment.get('target_kind') != 'paragraph':
        return comment

    word_range = comment.get('word_range')
    if not word_range:
        return comment

    content = comment.get('content', '')
    actual_word_count = count_words(content)

    # The word_range should be [start, end] where end <= actual_word_count
    # If the range is way larger than the content, it's likely character offsets
    start, end = word_range
    range_size = end - start

    # If the range suggests more than 3x the actual words, it's probably wrong
    # Also if start is 0 and end > actual_word_count, remove it
    if range_size > actual_word_count * 3 or (start == 0 and end > actual_word_count * 2):
        print(f"  Removing bad word_range {word_range} from paragraph {comment.get('paragraph_index')} "
              f"(content has ~{actual_word_count} words)")
        del comment['word_range']

    return comment

def main():
    if len(sys.argv) != 2:
        print("Usage: python3 fix_highlights.py <yaml_file>")
        sys.exit(1)

    yaml_file = sys.argv[1]

    print(f"Loading {yaml_file}...")
    with open(yaml_file, 'r') as f:
        comments = yaml.safe_load(f) or []

    print(f"Found {len(comments)} comments")

    # Fix each comment
    fixed_comments = [fix_comment(c) for c in comments]

    # Save with blank lines between entries
    print(f"\nSaving fixed comments to {yaml_file}...")
    with open(yaml_file, 'w') as f:
        for i, comment in enumerate(fixed_comments):
            if i > 0:
                f.write('\n')  # Add blank line before each entry except the first
            yaml.dump([comment], f, default_flow_style=False, allow_unicode=True, sort_keys=False)

    print("Done!")

if __name__ == '__main__':
    main()
