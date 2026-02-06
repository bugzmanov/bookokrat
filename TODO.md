PDF issue:
 [ ] BUG:PDF empty comments dismissal requires 2 ESC clicks
 [ ] BUG: EPUB https://registerspill.thorstenball.com/p/joy-and-curiosity-71 <-- some links are rendered next to text, without a space in between
 [ ] folded/unfolded state of context should be persisted in bookmarks.. maybe.
 [ ] pdf: zoom with pinching 
 [ ] Trim PDF margins mode
 [ ] DO not invert image colors mode
 [ ] links jumping with jump back is super unstable: after 1-2 successfull jumps back it looses capability of jumping (pdf links)
 [ ] MAYBE: copy the whole book: pdf & epub. 

Hard UX:
 [ ] Rework footer command bar to be context-aware and higher-contrast, reducing clutter.
 [ ] Comments editing in pdf annotations is very non-ergonomic. Potentially need normal mode.
 --
 [ ] update & analyze tapes & tests
 [ ] Analyze the PDF impact: size wise, heap wise and make it sure that it wouldn't affect epub only readers
 [ ] update help (new stuff & inspiration links)
    [ ] Make help context dependent
 [ ] new pictures for the site and potentially new video for the app. 

ideas to implement:
 - [ ] Dimming should probably just use math instead of sticking to fixed palette
 - [x] status bar to show errors and warnings
 - HTML Support & markdown
     [x] show raw html
         [x] Format html into readable
         [x] Search
         [ ] Navigate to the position where rendering mode was showing
         [x] Alow mouse selection & copy-paste
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
         [ ] checkboxes /-- not really needed to for epub
         [x] Image
         [ ] Code Coloring
         [x] epub:type blocks
 - AI integreration
     - Smart reading:
         - Chapter Summary and main points
         - Quize
     - Embeded validation for correct rendering (and markdown parsing)
     - Re-explain already explained term or abbreviation (like in chapter nine BFF might be frealy used as abbreviation, since it was introduced in chapter 1)
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
 - [x] User errors & info message
 - [x] NOTES & comments
     - [ ] Notes should support markdown (maybe)
     - [x] List/summary of comments
        - [x] Search in comments
 - [x] settings window
     - [x] make margins configurable + text color
     - [x] multiple themes
 - [x] text cursor for reading ala vim normal mode
 - [x] images in ghosty terminal
     - [x] images of various sized - right now bookokrat support just 2 presets
     - [ ] copy images to clipboard
 - [x] Images in iterm and sixt protocol
 - [x] ASCII Logo 
 - [ ] integration with GOODREADS

 - clickable scrollbar // not really needed

 - tmux + tape = to get claude to the position i want it to get to

bugs:
---------------------
[ ] Bug clicking in subchapter triggers image reload
[ ] Careless people: current chapter highlightning not working
[ ] Effective rust - chapter 4 
   [ ] Too much visual noise. (potentially because bg highlights)
[ ] <i>code</i> will have bg color instead of being cursive

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
