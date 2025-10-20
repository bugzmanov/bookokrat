ideas to implement:
 - [ ] Dimming should probably just use math instead of sticking to fixed palette
 - [ ] status bar to show errors and warnings
 - HTML Support & markdown
     [x] show raw html
         [ ] Format html into readable
         [ ] Navigate to the position where rendering mode was showing
         [ ] Alow mouse selection & stuff
     [x] Math Formulas
        [ ] Potential improvements: simple devision should be 1 line
     [ ] Markdown
         [.] links support
             [x] superscript links (footnotes)
             [ ] visited link tracking & styling
             [x] Internal links jumps history (to jump back)
             [x] external links
                [ ] bug: link in tables is not clickable
             [x] internal links
         [x] Horizontal rule
         [x] Headers
         [x] Blockquote
         [x] tables support
         [x] bold & italic
         [x] lists
         [ ] checkboxes /-- not really needs to for epub
         [x] Image
            [ ] according to logs kitty compression hapens too many times (maybe..)
         [ ] Code Coloring
         [x] epub:type blocks
 - AI integreration
     - Embeded validation for correct rendering (and markdown parsing)
     - Re-explain already explained term or abbreviation (like in chapter nine BFF might be frealy used as abbreviation, since it was introduced in chapter 1)
     - Smart reading:
         - Chapter Summary and main points
         - Quize
     - Phase 2: RAG Implementation
         - Build a local RAG system for your books:
         - Index entire library into embedded vectors
         - "Find passages about X across all my books"
         - "Show me similar concepts in other books"
         - Cross-reference technical books automatically

 - Search
     [x] Local to the chapter
     [x] Quick search with jumps in books and navigation panel
     [x] Global in the book
     [ ] Global in the library
 - [ ] Code formatting & coloring. Goal: Manning books should be nice to read
 - [ ] User errors & info message
 - [x] NOTES & comments
     - [ ] Notes should support markdown (maybe)
     - [ ] List of comments
        - [ ] Search in comments
 - [ ] settings window
     - [ ] make margins configurable + text color
 - [ ] text cursor for reading ala vim normal mode

 - images in ghosty terminal
     - images of various sized - needs larger size for large images
     - copy images to clipboard
 - [ ] Images in iterm and sixt protocol
 - [ ] integration with GOODREADS
 - ASCII art Logo & animations (eye candy)


 - clickable scrollbar

 - tmux + tape = to get claude to the position i want it to get to

bugs:
---------------------
[ ] Bug clicking in subchapter triggers image reaload
[ ] Currently reading chpater in TOC highlitning is seriously broken
[ ] Code block selection with mouse selects wrong lines
[ ] Adding comments doesn't move image down. it stays covering the text
[ ] Book search when jumps from the list to the book drops in the wrong location
[ ] comments are works with plain paragraphs and titles. but it's not properly working with all other text elements(like formulas or code blocks)
[ ] Machine Learning Q and A - links not working.
[ ] Careless people: current chanpter highlightning not workin

Tools with cool ratatui UI:
- https://github.com/erikjuhani/basalt
- https://github.com/benjajaja/mdfried  - render headers as images to have different scales.. don't know if i like it
- https://github.com/bgreenwell/doxx - docx reader



CLAUD ideas:
Phase 1: Local LLM Integration (Month 1-2)
    Intelligent Summarization: Shift+S generates chapter summaries using local LLaMA/Mistral
    Code Explanation: Hover over code snippets → get AI explanation in a popup
    Reading Comprehension: ? key opens Q&A mode about current page
    Smart Bookmarks: AI generates context-aware bookmark names


    Technical flex: Use Rust + FAISS/Qdrant for blazing-fast vector search

Phase 3: The "Learning Assistant" Features
    Adaptive Reading: AI adjusts complexity explanations based on your reading speed
    Knowledge Graph: Build connections between concepts across books
    Spaced Repetition: AI identifies key concepts and creates Anki-like reviews
    Reading Analytics: ML-based insights on your reading patterns

Use Rust + Candle (not Python):
Integrate with Modern AI Tools:

Ollama for local model management
ONNX runtime for optimized inference
Rust bindings for FAISS/Qdrant
WebGPU for GPU acceleration (cutting edge)

Done:
 - table of contents of a book
 - Mouse support:
   - scroll
   - select text
 - integration with GUI book reader
 - Recent books
     - reading history
     - drop dates from book reads. and instead make a separate list of most recent read books
 - SMALL IMAGES most likely needs to be ignored.
