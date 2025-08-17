ideas to implement:
 - images in ghosty terminal
     - images of various sized - needs larger size for large images
     - copy images to clipboard
 - HTML Support
     - show raw html
     - tables support
     - links support 
     - lists 
 - text cursor for reading ala vim normal mode
 - Code formatting & coloring. Goal: Manning books should be nice to read
 - AI integreration
 - logs/debug window
 - NOTES & comments
 - settings window
     - make margins configurable + text color 
 - Images in iterm and sixt protocol

 - integration with GOODREADS
 - ASCII art animation

bugz: 
ctrl+o - for opening is a bad idea, since this is usually "go back" in vim


CLAUD ideas:
Phase 1: Local LLM Integration (Month 1-2)
    Intelligent Summarization: Shift+S generates chapter summaries using local LLaMA/Mistral
    Code Explanation: Hover over code snippets → get AI explanation in a popup
    Reading Comprehension: ? key opens Q&A mode about current page
    Smart Bookmarks: AI generates context-aware bookmark names

Phase 2: RAG Implementation (Month 2-3)
    Build a local RAG system for your books:
    Index entire library into embedded vectors
    "Find passages about X across all my books"
    "Show me similar concepts in other books"
    Cross-reference technical books automatically

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
