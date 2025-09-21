ideas to implement:
 - Dimming should probably just use math instead of sticking to fixed palette
 - HTML Support & markdown
     [x] show raw html
         [ ] Format html into readable
         [ ] Navigate to the position where rendering mode was showing
         [ ] Alow mouse selection & stuff
     [x] Math Formulas 
        [ ] Potential improvements: simple devision should be 1 line
        [ ] Bug: multiline math in lists
     [ ] Markdown
         [.] links support 
             [x] superscript links (footnotes)
             [ ] visited link tracking & styling
             [ ] Internal links jumps history (to jump back)
             [x] external links
                [ ] bug: link in tables is not clickable
             [x] internal links
         [ ] Horizontal rule
         [x] Headers
         [ ] Blockquote
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

[ ] Bug clicking in subchapter triggers image reaload
Intermittent issues
    [ ] BUG: copy-paste copies wrong block & word selection(double click) and paragraph selection(triple click) got broken
    
 - Search
     [x] Local to the chapter
     [x] Quick search with jumps in books and navigation panel
     [ ] Global in the book
     [ ] Global in the library
 - Code formatting & coloring. Goal: Manning books should be nice to read
 - logs/debug window
 - NOTES & comments
 - settings window
     - make margins configurable + text color 
 - text cursor for reading ala vim normal mode

 - images in ghosty terminal
     - images of various sized - needs larger size for large images
     - copy images to clipboard
 - Images in iterm and sixt protocol
 - integration with GOODREADS
 - ASCII art animation

Ideas to properly try:
 - Markdown proper parsing instead of regexp (like in basalt)


bugs: 
---------------------
Nested chapter should have better representation (now they are treate like one blob) 
Internal links is not supported yet
ctrl+o - for opening is a bad idea, since this is usually "go back" in vim
without code block support comments like # are translated to header



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
