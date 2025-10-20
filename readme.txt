===============================================================================

                              FEATURES AT A GLANCE

  [LIBRARY]
    ▸ Automatic EPUB discovery in current directory
    ▸ Split-view interface with library browser and reader
    ▸ Hierarchical table of contents with expandable sections
    ▸ Automatic bookmarks - resume exactly where you left off
    ▸ Reading history with quick access to recent books

  [READING]
    ▸ Full MathML rendering with ASCII art conversion
    ▸ Embedded image display with zoom popup viewer
    ▸ Syntax-highlighted code blocks
    ▸ Formatted tables rendered for terminal
    ▸ Raw HTML view for debugging
    ▸ Reading progress tracking with time estimates

  [SEARCH & NAVIGATION]
    ▸ Chapter-level search with fuzzy matching
    ▸ Book-wide search across all chapters
    ▸ Vim-style jump list (Ctrl+o/Ctrl+i)
    ▸ Internal anchor following with breadcrumb trail
    ▸ Quick chapter-to-chapter navigation

  [ANNOTATIONS]
    ▸ Text selection with mouse or keyboard
    ▸ Inline comments on selected passages
    ▸ Copy text snippets or entire chapters
    ▸ Selection modes: word, paragraph, custom range

  [POWER USER]
    ▸ Vim-like keybindings throughout
    ▸ Full keyboard and mouse control
    ▸ External EPUB reader integration
    ▸ Performance profiling overlay
    ▸ Book statistics popup

===============================================================================

                            KEYBOARD REFERENCE CARD

┌─────────────────────────────────────────────────────────────────────────────┐
│ GLOBAL CONTROLS                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  q             Quit application                                             │
│  Tab           Switch focus between library and reader                      │
│  Esc           Clear selection, exit search, dismiss popups                 │
│  Space+h       Toggle reading history popup                                 │
│  Space+d       Show book statistics popup                                   │
│  Space+o       Open current book in system EPUB viewer                      │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ LIBRARY & TABLE OF CONTENTS PANEL                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Move down / up                                               │
│  Ctrl+d / u    Scroll half page down / up                                   │
│  gg            Jump to top                                                  │
│  G             Jump to bottom                                               │
│  /             Start search/filter                                          │
│  n / N         Next / previous search match                                 │
│  h / l         Collapse / expand TOC entry                                  │
│  H / L         Collapse / expand all entries                                │
│  Enter         Open highlighted book or chapter                             │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ READER PANEL - SCROLLING & NAVIGATION                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Scroll down / up by line                                     │
│  Ctrl+d / u    Scroll half screen down / up                                 │
│  gg            Jump to top of chapter                                       │
│  G             Jump to bottom of chapter                                    │
│  h / l         Previous / next chapter                                      │
│  Ctrl+o        Jump backward in history                                     │
│  Ctrl+i        Jump forward in history                                      │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ READER PANEL - SEARCH                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  /             Search within current chapter                                │
│  n / N         Next / previous search result                                │
│  Space+f       Reopen last book-wide search                                 │
│  Space+F       Start fresh book-wide search                                 │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ READER PANEL - TEXT & CONTENT                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│  c / Ctrl+C    Copy selected text                                           │
│  Space+c       Copy entire chapter                                          │
│  Space+z       Copy debug transcript                                        │
│  a             Add/edit comment on selection                                │
│  d             Delete comment under cursor                                  │
│  Space+s       Toggle raw HTML view                                         │
│  Enter         Open image popup (when cursor on image)                      │
│  p             Toggle performance profiler overlay                          │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ BOOK SEARCH POPUP (Space+f / Space+F)                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  Type          Search entire book                                           │
│  Enter         Execute search or jump to result                             │
│  j / k         Navigate results                                             │
│  g / G         Jump to top / bottom of results                              │
│  Space         Return to search input field                                 │
│  Esc           Close popup                                                  │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ READING HISTORY POPUP (Space+h)                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Navigate entries                                             │
│  Ctrl+d / u    Scroll page down / up                                        │
│  gg / G        Jump to top / bottom                                         │
│  Enter         Open selected book                                           │
│  Esc           Close popup                                                  │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ BOOK STATISTICS POPUP (Space+d)                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Navigate chapters                                            │
│  Ctrl+d / u    Scroll page down / up                                        │
│  gg / G        Jump to top / bottom                                         │
│  Enter         Jump to selected chapter                                     │
│  Esc           Close popup                                                  │
└─────────────────────────────────────────────────────────────────────────────┘

===============================================================================

                                 MOUSE SUPPORT

Bookokrat provides full mouse integration:

  [PANELS]
    • Scroll wheel - Scroll content (smooth scrolling)
    • Single-click - Focus a panel
    • Double-click (library) - Open selected book
    • Double-click (reader) - Select word under cursor
    • Triple-click (reader) - Select entire paragraph

  [TEXT SELECTION]
    • Click-and-drag - Highlight text
    • Drag past edges - Auto-scroll viewport
    • Release on link - Follow hyperlink

  [IMAGES & INTERACTIVE]
    • Click image - Open in zoom popup
    • Click popup - Dismiss (or press any key)
    • Click history/stats entry - Activate immediately

===============================================================================

                            COMMENTS & ANNOTATIONS

Add notes directly to your books:

  [1] Select text using mouse (click-and-drag) or keyboard
  [2] Press 'a' to create or edit a comment
  [3] Type your note in the popup editor
  [4] Press Esc to save the comment
  [5] Press 'd' when on a commented passage to delete it

Comments are saved per-book and persist across sessions.

===============================================================================

                              ADVANCED FEATURES

  [LINK NAVIGATION]
    Following links creates a navigation breadcrumb trail. Use Ctrl+o and
    Ctrl+i to jump backward and forward through your reading path, just
    like in vim.

  [EXTERNAL READER INTEGRATION]
    Press Space+o to hand off the current book to your system's EPUB reader.
    Bookokrat detects and supports:
      • macOS: Calibre, ClearView, Skim
      • Linux: Calibre, FBReader
      • Windows: Calibre

  [PERFORMANCE PROFILING]
    Press 'p' to toggle the performance profiler overlay:
      • FPS (frames per second)
      • Frame timing statistics
      • Rendering performance metrics

===============================================================================

                                TIPS & TRICKS

  ▸ Fast chapter navigation: Use h/l in reader to jump between chapters
  ▸ Quick book switching: Press Space+h for recent books
  ▸ Search workflow: Use / for chapter searches, Space+F for book-wide
  ▸ Reading statistics: Press Space+d to see chapter counts and progress
  ▸ Debug view: Press Space+s to toggle raw HTML for rendering issues
  ▸ Smooth scrolling: Hold j or k for accelerated scrolling
  ▸ Half-page jumps: Use Ctrl+d and Ctrl+u with visual highlights

===============================================================================

                              PLATFORM SUPPORT

Bookokrat runs on:
  • macOS (tested on 10.15+)
  • Linux (tested on Ubuntu, Debian, Arch)
  • Windows (tested on Windows 10/11)

Terminal requirements:
  • True color support recommended
  • UTF-8 encoding
  • Mouse event support (most modern terminals)

===============================================================================

                     Made with Rust 🦀 for Terminal Lovers

===============================================================================
