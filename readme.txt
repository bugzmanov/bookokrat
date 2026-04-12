===============================================================================

                              FEATURES AT A GLANCE

  [LIBRARY]
    ▸ Automatic EPUB / PDF discovery in current directory
    ▸ Calibre library detection with metadata (titles, authors)
    ▸ Libraries with multiple source directories
    ▸ EPUB bundle support (exploded .epub directories, Apple Books exports)
    ▸ Split-view interface with resizable library browser and reader
    ▸ Hierarchical table of contents with expandable sections
    ▸ Automatic bookmarks - resume exactly where you left off
    ▸ Reading history with quick access to recent books (global across libraries)
    ▸ Reading progress tracking per book

  [READING]
    ▸ EPUB: rich rendering (MathML, code, tables, images, links)
    ▸ EPUB: adjustable content margins and text justification
    ▸ PDF: true rendering with page/scroll modes + zoom (graphics terminal)
    ▸ PDF: single/dual page layout and scroll/page mode switchers
    ▸ PDF: auto-reload on file change (file watching)
    ▸ PDF: quick jump to page by number
    ▸ PDF: TOC detection with printed page mapping
    ▸ Smooth scrolling + multiclick selection
    ▸ Zen mode for distraction-free reading

  [SEARCH & NAVIGATION]
    ▸ Chapter-level search with fuzzy matching
    ▸ Book-wide search across all chapters
    ▸ Vim-style jump list (Ctrl+o/Ctrl+i)
    ▸ Internal anchor following with breadcrumb trail
    ▸ Quick chapter-to-chapter navigation

  [ANNOTATIONS]
    ▸ Text selection with mouse or keyboard
    ▸ Inline comments on selected passages (EPUB/PDF)
    ▸ Export annotations to Markdown
    ▸ Copy text snippets or entire chapters
    ▸ Selection modes: word, paragraph, custom range

  [SYNCTEX - LaTeX ↔ PDF]
    ▸ Bidirectional sync between LaTeX source and PDF output
    ▸ Inverse search: Ctrl+click, right-click, or gd → jump to source
    ▸ Forward search: editor sends line:col:file → PDF scrolls to match
    ▸ Unix socket IPC for live editor integration (VimTeX, etc.)
    ▸ Configurable editor command with {file}, {line}, {column} placeholders

  [POWER USER]
    ▸ Vim-like keybindings throughout
    ▸ Reader normal mode with motions, counts, visual selection, and yanks
    ▸ Full keyboard and mouse control
    ▸ External EPUB reader integration
    ▸ Performance profiling overlay (optional `profile` feature)
    ▸ Book statistics popup
    ▸ Settings popup for PDF support and render mode (Space+s / Ctrl+s)
    ▸ Multiple built-in color themes (Oceanic Next, Catppuccin, Kanagawa)

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
│  Space+s       Open settings (PDF support + render mode)                    │
│  + / -         Increase / decrease content margins                          │
│  Space+j       Toggle text justification (EPUB)                             │
│  Space+h       Toggle reading history popup                                 │
│  Space+d       Show book statistics popup                                   │
│  Space+o       Open current book in system viewer                           │
│  Space+l       Lookup selected text (run configured shell command)          │
│  Space+w       Toggle file watching / auto-reload (PDF)                     │
│  Space+D       Toggle single / dual page layout (PDF)                       │
│  Space+S       Toggle scroll / page render mode (PDF, Kitty only)           │
│  Space+g       Jump to page number (PDF)                                    │
│  Space+a       Open comments/annotations viewer                             │
│  < / >         Shrink / grow sidebar panel width                            │
│  Space+< / >   Reset sidebar panel width to default                         │
│  Ctrl+l        Force full screen redraw                                     │
│  Ctrl+q        Suspend process (unix only, resume with fg)                  │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│ LIBRARY & TABLE OF CONTENTS PANEL                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│  j / k         Move down / up                                               │
│  Ctrl+d / u    Scroll half page down / up                                   │
│  Ctrl+f / b    Scroll full page down / up                                   │
│  PgDn / PgUp   Scroll full page down / up                                   │
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
│  Ctrl+f / b    Scroll full screen down / up                                 │
│  PgDn / PgUp   Scroll full screen down / up                                 │
│  gg            Jump to top of chapter                                       │
│  G             Jump to bottom of chapter                                    │
│  { / }         Jump to previous / next paragraph                            │
│  h / l         Previous / next chapter                                      │
│  Ctrl+o        Jump backward in history                                     │
│  Ctrl+i        Jump forward in history                                      │
│  [n]gg (PDF)   Jump to page number n (e.g. 42gg)                            │
│  z (PDF)       Zoom to fit height                                           │
│  Z (PDF)       Zoom to fit width                                            │
│  i (PDF)       Toggle image inversion (themed mode only; saved per book)    │
│  I (PDF)       Switch between original PDF rendering and themed style       │
│                (saved per book).                                            │
│                Themed style is the default.                                 │
│                In original rendering mode, i has no visual effect.          │
│  n             Toggle normal mode                                           │
│  gd (PDF)      SyncTeX inverse search: jump to LaTeX source at cursor       │
│  Ctrl+click    SyncTeX inverse search at click position (PDF)               │
│  Right-click   SyncTeX inverse search at click position (PDF)               │
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
│  Ctrl+d/u/f/b  Half / full page scroll                                      │
│  { / }         Jump to previous / next paragraph                            │
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
│  Space+c       Copy chapter (EPUB) / extract page text (PDF)                │
│  Space+C       Copy TOC selection (PDF, requires TOC focus)                 │
│  Space+z       Copy debug transcript                                        │
│  a             Add/edit comment on selection                                │
│  d             Delete comment under cursor                                  │
│  ss            Toggle raw HTML view (EPUB/HTML only)                        │
│  Enter         Open image popup (when cursor on image)                      │
│  p             Toggle performance profiler overlay (`profile` builds)       │
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
│ READING HISTORY POPUP (Space+h) - global across all libraries               │
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
    • Drag panel border - Resize sidebar width
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

Comments are saved per-book in your XDG data directory and persist across
sessions. Your working directory stays clean.
PDF annotations require a graphics-capable terminal.

===============================================================================

                              ADVANCED FEATURES

  [SYNCTEX - LaTeX ↔ PDF SYNCHRONIZATION]
    Bookokrat supports bidirectional SyncTeX navigation for LaTeX workflows.
    Compile with synctex enabled (e.g. pdflatex --synctex=1) so a .synctex.gz
    sidecar is generated alongside the PDF. SyncTeX activates automatically
    when a matching sidecar is found.

    Inverse search (PDF → source):
      Ctrl+click, right-click on the PDF, or gd in normal mode jumps to
      the corresponding LaTeX source line. gd also works on selected text
      (mouse selection anchor is used). Configure which editor opens
      in ~/.bookokrat_settings.yaml or in the Settings popup (Space+s):

        synctex_editor: "nvim --server /tmp/nvim.sock --remote-send '<C-\><C-n>:e {file}<CR>:{line}<CR>'"

      Placeholders: {file}, {line}, {column}

    Forward search (source → PDF):
      From your editor, run:

        bookokrat --synctex-forward LINE:COLUMN:FILE path/to/document.pdf

      This sends a command over a Unix socket to the running instance,
      which scrolls the PDF to the matching position.

      VimTeX setup (add to your init.vim / init.lua):

        let g:vimtex_view_method = 'general'
        let g:vimtex_view_general_cmd = 'bookokrat --synctex-forward @line:@col:@tex @pdf'

      Then press \lv in Neovim to jump from source to PDF.

      Generic editor setup — any editor that can shell out works:

        bookokrat --synctex-forward 42:0:main.tex document.pdf

    The title bar shows [SyncTeX] when active and [SyncTeX: watching]
    when both SyncTeX and file watching are enabled.

  [LINK NAVIGATION]
    Following links creates a navigation breadcrumb trail. Use Ctrl+o and
    Ctrl+i to jump backward and forward through your reading path, just
    like in vim.

  [EXTERNAL READER INTEGRATION]
    Press Space+o to hand off the current book to your system viewer.
    Bookokrat detects and supports:
      • macOS: Calibre, ClearView, Skim
      • Linux: Calibre, FBReader
      • Windows: Calibre

  [PERFORMANCE PROFILING]
    Press 'p' to toggle the performance profiler overlay in `profile` builds:
      • FPS (frames per second)
      • Frame timing statistics
      • Rendering performance metrics

===============================================================================

                                TIPS & TRICKS

  ▸ Fast chapter navigation: Use h/l in reader to jump between chapters
  ▸ Quick book switching: Press Space+h for recent books
  ▸ Search workflow: Use / for chapter searches, Space+F for book-wide
  ▸ Reading statistics: Press Space+d to see chapter counts and progress
  ▸ Debug view: Press ss to toggle raw HTML for rendering issues (EPUB)
  ▸ PDF settings: Press Space+s (or Ctrl+s) to toggle support and render mode
  ▸ Smooth scrolling: Hold j or k for accelerated scrolling
  ▸ Half-page jumps: Use Ctrl+d and Ctrl+u with visual highlights
  ▸ Full-page jumps: Use Ctrl+f / Ctrl+b or PgDn / PgUp
  ▸ Focus reading: Press Ctrl+z for zen mode (hides panels)
  ▸ Adjust margins: Press + or - to widen or narrow content
  ▸ Theme switching: Press Space+t to browse and apply color themes

===============================================================================

                               CUSTOMIZATION

  [DATA STORAGE]
    Bookokrat stores all application data in XDG-compliant directories,
    keeping your working directories clean. Each directory where you run
    bookokrat is treated as an isolated library:

      • Bookmarks    <data_dir>/bookokrat/libraries/<library>/bookmarks.json
      • Comments     <data_dir>/bookokrat/libraries/<library>/comments/
      • Image cache  <cache_dir>/bookokrat/libraries/<library>/temp_images/
      • Log file     <state_dir>/bookokrat/bookokrat.log

    Typical <data_dir> paths:
      • macOS:  ~/Library/Application Support
      • Linux:  ~/.local/share

    If old files (bookmarks.json, .bookokrat_comments/, temp_images/) are
    found in your working directory, they are automatically migrated to
    the new locations on startup.

  [SETTINGS FILE]
    Bookokrat saves your preferences to ~/.bookokrat_settings.yaml:
      • Selected theme
      • Content margin setting
      • Custom color themes
      • PDF enabled flag and render mode
      • PDF scale and pan shift

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

  [DICTIONARY / SHELL LOOKUP]
    Select text and press Space+l to look up words. Configure in your
    settings file (~/.bookokrat_settings.yaml):

    Console dictionary (output shown in scrollable popup):
      lookup_command: "dict {}"
      lookup_display: popup
      Other options: sdcv -n {} (offline), wn {} -over (WordNet)

    Web dictionary (opens in browser):
      lookup_command: "open 'https://www.merriam-webster.com/dictionary/{}'"
      lookup_display: fire_and_forget

    GUI dictionary (opens native app):
      lookup_command: "open dict://{}"
      lookup_display: fire_and_forget
      Other options: goldendict {} (cross-platform)

    {} is replaced with the selected text. lookup_display can be "popup"
    (capture output) or "fire_and_forget" (launch and forget).

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
  • Graphics protocol required for PDF viewing (Kitty/Ghostty/WezTerm/iTerm2)

===============================================================================

                     Made with Rust 🦀 for Terminal Lovers

===============================================================================
