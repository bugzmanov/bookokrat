#!/usr/bin/env python3
"""
Extract raw table of contents from EPUB file.
Outputs the original NCX or NAV document as-is.
"""

import sys
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path

def extract_toc_from_epub(epub_path):
    """Extract raw TOC from EPUB file."""
    
    with zipfile.ZipFile(epub_path, 'r') as epub:
        # First, find the container.xml to locate the OPF file
        try:
            container_xml = epub.read('META-INF/container.xml').decode('utf-8')
            container_root = ET.fromstring(container_xml)
            
            # Find the OPF file path
            ns = {'ns': 'urn:oasis:names:tc:opendocument:xmlns:container'}
            rootfile = container_root.find('.//ns:rootfile', ns)
            opf_path = rootfile.get('full-path')
            
            print(f"Found OPF at: {opf_path}")
            print("=" * 80)
            
        except Exception as e:
            print(f"Error reading container.xml: {e}")
            return
        
        # Read the OPF file to find TOC references
        try:
            opf_content = epub.read(opf_path).decode('utf-8')
            opf_root = ET.fromstring(opf_content)
            
            # Get the base directory of the OPF file
            opf_dir = str(Path(opf_path).parent)
            if opf_dir == '.':
                opf_dir = ''
            else:
                opf_dir = opf_dir + '/'
            
        except Exception as e:
            print(f"Error reading OPF file: {e}")
            return
        
        # Look for NCX file (EPUB2)
        ncx_found = False
        for item in opf_root.iter():
            if item.tag.endswith('item'):
                media_type = item.get('media-type', '')
                href = item.get('href', '')
                item_id = item.get('id', '')
                
                if 'ncx' in media_type or href.endswith('.ncx'):
                    ncx_path = opf_dir + href
                    print(f"Found NCX file: {ncx_path}")
                    print("-" * 80)
                    try:
                        ncx_content = epub.read(ncx_path).decode('utf-8')
                        print(ncx_content)
                        ncx_found = True
                        print("=" * 80)
                    except Exception as e:
                        print(f"Error reading NCX: {e}")
        
        # Look for NAV file (EPUB3)
        nav_found = False
        for item in opf_root.iter():
            if item.tag.endswith('item'):
                properties = item.get('properties', '')
                href = item.get('href', '')
                
                if 'nav' in properties or 'nav' in href.lower() or 'toc' in href.lower():
                    nav_path = opf_dir + href
                    
                    # Skip if we already printed this as NCX
                    if nav_path.endswith('.ncx'):
                        continue
                        
                    print(f"Found NAV file: {nav_path}")
                    print("-" * 80)
                    try:
                        nav_content = epub.read(nav_path).decode('utf-8')
                        print(nav_content)
                        nav_found = True
                        print("=" * 80)
                    except Exception as e:
                        print(f"Error reading NAV: {e}")
        
        if not ncx_found and not nav_found:
            print("No TOC files found in EPUB")
            print("\nAvailable files in EPUB:")
            for file_info in epub.filelist:
                if 'toc' in file_info.filename.lower() or 'nav' in file_info.filename.lower() or 'ncx' in file_info.filename.lower():
                    print(f"  - {file_info.filename}")

def main():
    if len(sys.argv) != 2:
        print("Usage: python extract_toc_raw.py <epub_file>")
        sys.exit(1)
    
    epub_path = sys.argv[1]
    
    if not Path(epub_path).exists():
        print(f"Error: File '{epub_path}' not found")
        sys.exit(1)
    
    if not epub_path.lower().endswith('.epub'):
        print(f"Warning: File '{epub_path}' doesn't have .epub extension")
    
    print(f"Extracting TOC from: {epub_path}")
    print("=" * 80)
    
    extract_toc_from_epub(epub_path)

if __name__ == "__main__":
    main()