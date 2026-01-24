===============================================================================

                              FEATURES AT A GLANCE

  [LIBRARY]
    ▸ Automatic EPUB discovery in current directory
    ▸ Calibre library detection with metadata (titles, authors)
    ▸ EPUB bundle support (exploded .epub directories, Apple Books exports)
    ▸ Split-view interface with library browser and reader
    ▸ Hierarchical table of contents with expandable sections
    ▸ Automatic bookmarks - resume exactly where you left off
    ▸ Reading history with quick access to recent books

  [READING]
    ▸ Full MathML rendering with ASCII art conversion
    ▸ Embedded image display with zoom popup viewer
    ▸ Syntax-highlighted code blocks
    ▸ Advanced table rendering with rich cell content
    ▸ Raw HTML view for debugging
    ▸ Reading progress tracking with time estimates
    ▸ Auto-selected image protocol per terminal (Kitty/Sixel/iTerm2/Halfblocks)

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
    ▸ Reader normal mode with motions, counts, visual selection, and yanks
    ▸ Full keyboard and mouse control
    ▸ External EPUB reader integration
    ▸ Performance profiling overlay
    ▸ Book statistics popup

  [CUSTOMIZATION]
    ▸ Multiple built-in color themes (Oceanic Next, Catppuccin, Kanagawa)
    ▸ Custom theme support via Base16 color schemes
    ▸ Adjustable content margins
    ▸ Zen mode - distraction-free reading
    ▸ Persistent settings across sessions (~/.bookokrat_settings.yaml)

===============================================================================

                            KEYBOARD REFERENCE CARD

┌─────────────────────────────────────────────────────────────────────────────┐
│ GLOBAL CONTROLS                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  q             Quit application                                             │
│  Ctrl+z        Toggle zen mode (hide sidebar and status bar)                │
│  Tab           Switch focus between library and reader                      │
│  Esc           Clear selection, exit search, dismiss popups                 │
│  ?             Toggle this help screen                                      │
│  Space+t       Open theme selector                                          │
│  + / -         Increase / decrease content margins                          │
│  Space+h       Toggle reading history popup                                 │
│  Space+d       Show book statistics popup                                   │
│  Space+o       Open current book in system EPUB viewer                      │
│  Space+a       Open comments/annotations viewer                             │
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
│  n             Toggle normal mode                                           │
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
│ READER PANEL - NORMAL MODE (press n to enter/exit)                          │
├─────────────────────────────────────────────────────────────────────────────┤
│  h/j/k/l       Move cursor                                                  │
│  w/W/b/e       Word motions (small/Big)                                     │
│  0/^/$         Line start / first non-space / end                           │
│  gg / G        Top / bottom of document                                     │
│  f/F/t/T ;     Find/till char and repeat                                    │
│  v / V         Visual selection (char/line)                                 │
│  y             Yank (copy) with motions/objects or visual selection         │
│  Enter         Open link under cursor                                       │
│  Esc           Exit visual mode                                             │
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

┌─────────────────────────────────────────────────────────────────────────────┐
│ COMMENTS VIEWER (Space+a)                                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│  Tab           Switch focus between chapter list and comments pane          │
│  j / k         Navigate entries in focused pane                             │
│  h / l         Jump to previous / next chapter (in comments pane)           │
│  /             Search within current scope                                  │
│  ?             Toggle global search mode (search all comments)              │
│  Enter         Jump to comment location in reader                           │
│  dd            Delete highlighted comment                                   │
│  Space+e       Export all annotations to Markdown file                      │
│  Esc           Close viewer                                                 │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ THEME SELECTOR (Space+t)                                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Navigate themes                                              │
│  Enter         Apply selected theme                                         │
│  Esc           Close without changing                                       │
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

Code block annotations:

  ▸ Click on a code block line to position cursor
  ▸ Press 'a' to annotate a single line or selected range
  ▸ Line-specific comments display next to the code

Review and manage notes efficiently:

  ▸ Space+a opens the two-pane comments viewer
  ▸ Left pane lists chapters and comment counts; right pane shows notes
  ▸ Tab toggles focus between panes; mouse wheel scrolls the pane you hover
  ▸ h / l jump to previous / next chapter while keeping the comments focus
  ▸ ? (Shift+/) switches to global search mode to scan every comment at once
  ▸ / searches within the current scope (chapter or global)
  ▸ Enter or double-click jumps from a comment back into the reader
  ▸ dd deletes the highlighted comment directly from the viewer

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
  ▸ Focus reading: Press Ctrl+z for zen mode (hides panels)
  ▸ Adjust margins: Press + or - to widen or narrow content
  ▸ Theme switching: Press Space+t to browse and apply color themes

===============================================================================

                               CUSTOMIZATION

  [SETTINGS FILE]
    Bookokrat saves your preferences to ~/.bookokrat_settings.yaml:
      • Selected theme
      • Content margin setting
      • Custom color themes

    Settings persist across sessions and apply to all book directories.

  [COLOR THEMES]
    Built-in themes:
      • Oceanic Next (default)
      • Catppuccin Mocha
      • Kanagawa
      • Kanagawa Dragon

    Add custom themes using Base16 color schemes. Edit your settings file
    and add entries to the custom_themes section. See the commented template
    in the settings file for the full color format.

  [ZEN MODE]
    Press Ctrl+z to toggle zen mode for distraction-free reading:
      • Hides the sidebar (library/TOC panel)
      • Hides the status bar
      • Maximizes the reading area

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
