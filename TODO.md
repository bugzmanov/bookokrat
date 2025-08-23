ideas to implement:
 - HTML Support & markdown
     [x] show raw html
         [ ] Format html into readable
         [ ] Navigate to the position where rendering mode was showing
         [ ] Alow mouse selection & stuff
     [ ] Math Formulas
     [ ] Markdown
         [.] links support 
             [x] external links
             [ ] internal links
         [ ] Horizontal rule
         [x] Headers
           [ ] H1 getting removed bug  (because special treatment of headers that i had before)
         [ ] Blockquote
         [ ] tables support
         [ ] bold & italic 
         [x] lists
           [ ] Nested lists are buggy
           [ ] Links inside lists get broken
         [ ] checkboxes /-- not really needs to for epub
         [x] Image
         [ ] Code & "render as is"
 - AI integreration
     - Embeded validation for correct rendering (and markdown parsing) 
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
     - Local to the chapter
     - Quick search with jumps in books and navigation panel
     - Global in the book
     - Globali in the library
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
