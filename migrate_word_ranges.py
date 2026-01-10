#!/usr/bin/env python3
"""
Script to fix word_range values in bookokrat comments by extracting the actual
text from the EPUB and recalculating word positions.
"""

import yaml
import sys
from pathlib import Path
import zipfile
from html.parser import HTMLParser


class TextExtractor(HTMLParser):
    """Extract plain text from HTML."""
    def __init__(self):
        super().__init__()
        self.text_parts = []
        self.skip_tags = {'script', 'style', 'head'}
        self.current_tag = None

    def handle_starttag(self, tag, attrs):
        if tag.lower() in self.skip_tags:
            self.current_tag = tag
        _ = attrs  # Unused but required by interface

    def handle_endtag(self, tag):
        if tag.lower() == self.current_tag:
            self.current_tag = None

    def handle_data(self, data):
        if not self.current_tag:
            self.text_parts.append(data)

    def get_text(self):
        return ''.join(self.text_parts)


def extract_epub_chapter(epub_path, chapter_href):
    """Extract text content from an EPUB chapter."""
    try:
        with zipfile.ZipFile(epub_path, 'r') as epub:
            # Try common paths
            possible_paths = [
                chapter_href,
                f'OEBPS/{chapter_href.split("/")[-1]}',
                chapter_href.replace('OEBPS/', '')
            ]

            for path in possible_paths:
                try:
                    html_content = epub.read(path).decode('utf-8')
                    parser = TextExtractor()
                    parser.feed(html_content)
                    return parser.get_text()
                except KeyError:
                    continue

            print(f"  WARNING: Could not find {chapter_href} in EPUB")
            return None
    except Exception as e:
        print(f"  ERROR extracting chapter: {e}")
        return None


def count_words(text):
    """Count words in text, matching Rust implementation."""
    word_count = 0
    in_word = False

    for ch in text:
        if ch.isspace():
            in_word = False
        elif not in_word:
            word_count += 1
            in_word = True

    return word_count


def find_text_in_paragraph(paragraph_text, search_text):
    """
    Find the word range of search_text within paragraph_text.
    Returns (start_word, end_word) or None if not found.
    """
    # Normalize whitespace
    para_normalized = ' '.join(paragraph_text.split())
    search_normalized = ' '.join(search_text.split())

    # Try to find the text
    if search_normalized not in para_normalized:
        # Try case-insensitive
        para_lower = para_normalized.lower()
        search_lower = search_normalized.lower()
        if search_lower not in para_lower:
            return None
        char_pos = para_lower.index(search_lower)
    else:
        char_pos = para_normalized.index(search_normalized)

    # Convert character position to word index
    words_before = count_words(para_normalized[:char_pos])
    words_in_selection = count_words(para_normalized[char_pos:char_pos + len(search_normalized)])

    return (words_before, words_before + words_in_selection)


def split_into_paragraphs(text):
    """Split text into paragraphs."""
    # Split on double newlines or significant whitespace
    paragraphs = []
    current = []

    for line in text.split('\n'):
        line = line.strip()
        if line:
            current.append(line)
        elif current:
            paragraphs.append(' '.join(current))
            current = []

    if current:
        paragraphs.append(' '.join(current))

    return paragraphs


def fix_comment_with_epub(comment, epub_path):
    """Fix a comment's word_range by extracting text from the EPUB."""
    # Only process paragraph comments with word_range
    if comment.get('target_kind') != 'paragraph':
        return comment, False

    word_range = comment.get('word_range')
    if not word_range:
        return comment, False

    chapter_href = comment.get('chapter_href')
    paragraph_index = comment.get('paragraph_index')
    selected_text = comment.get('selected_text')

    # If no selected_text was saved, we can't fix this accurately
    if not selected_text:
        # Check if word_range looks suspicious (too large)
        _start, end = word_range
        if end > 200:  # Likely character count, not word count
            print(f"  Paragraph {paragraph_index}: No selected_text saved, removing suspicious word_range {word_range}")
            del comment['word_range']
            return comment, True
        return comment, False

    # Extract chapter content
    chapter_text = extract_epub_chapter(epub_path, chapter_href)
    if not chapter_text:
        return comment, False

    # Split into paragraphs
    paragraphs = split_into_paragraphs(chapter_text)

    if paragraph_index >= len(paragraphs):
        print(f"  WARNING: Paragraph index {paragraph_index} out of range (only {len(paragraphs)} paragraphs)")
        return comment, False

    paragraph_text = paragraphs[paragraph_index]

    # Find the selected text in the paragraph
    new_word_range = find_text_in_paragraph(paragraph_text, selected_text)

    if new_word_range:
        old_range = tuple(word_range)
        if old_range != new_word_range:
            print(f"  Paragraph {paragraph_index}: Fixed word_range from {old_range} to {new_word_range}")
            comment['word_range'] = list(new_word_range)
            return comment, True
        else:
            print(f"  Paragraph {paragraph_index}: word_range {old_range} already correct")
            return comment, False
    else:
        print(f"  WARNING: Could not find selected text in paragraph {paragraph_index}")
        print(f"    Selected: {selected_text[:100]}...")
        print(f"    Paragraph: {paragraph_text[:100]}...")
        return comment, False


def main():
    if len(sys.argv) != 3:
        print("Usage: python3 migrate_word_ranges.py <yaml_file> <epub_file>")
        sys.exit(1)

    yaml_file = sys.argv[1]
    epub_file = sys.argv[2]

    if not Path(yaml_file).exists():
        print(f"ERROR: YAML file not found: {yaml_file}")
        sys.exit(1)

    if not Path(epub_file).exists():
        print(f"ERROR: EPUB file not found: {epub_file}")
        sys.exit(1)

    print(f"Loading comments from {yaml_file}...")
    with open(yaml_file, 'r') as f:
        comments = yaml.safe_load(f) or []

    print(f"Found {len(comments)} comments\n")
    print(f"Extracting text from {epub_file}...\n")

    # Fix each comment
    fixed_comments = []
    changed_count = 0

    for i, comment in enumerate(comments):
        fixed_comment, was_changed = fix_comment_with_epub(comment, epub_file)
        fixed_comments.append(fixed_comment)
        if was_changed:
            changed_count += 1

    print(f"\n{'='*60}")
    print(f"Fixed {changed_count} out of {len(comments)} comments")
    print(f"{'='*60}\n")

    # Backup original file
    backup_file = yaml_file + '.backup'
    print(f"Creating backup at {backup_file}")
    with open(backup_file, 'w') as f:
        with open(yaml_file, 'r') as orig:
            f.write(orig.read())

    # Save fixed comments with blank lines between entries
    print(f"Saving fixed comments to {yaml_file}...")
    with open(yaml_file, 'w') as f:
        for i, comment in enumerate(fixed_comments):
            if i > 0:
                f.write('\n')  # Add blank line before each entry
            yaml.dump([comment], f, default_flow_style=False, allow_unicode=True, sort_keys=False)

    print("Done!")
    print(f"\nOriginal file backed up to: {backup_file}")


if __name__ == '__main__':
    main()
