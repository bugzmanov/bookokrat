#!/usr/bin/env python3
"""
Create a minimal test PDF with the exact TOC structure from "Dependency Injection"
by Mark Seemann and Steven van Deursen.

This creates a PDF with:
- The same outline/bookmark structure (titles, hierarchy levels, page targets)
- Minimal page content (just page numbers)
- No copyrighted content

Used for regression testing TOC extraction.
"""

from reportlab.lib.pagesizes import letter
from reportlab.pdfgen import canvas
from reportlab.lib.units import inch

# Exact TOC structure from "Dependency Injection" book
# Format: (title, level, page) - page is 0-indexed
TOC_ENTRIES = [
    ("Dependency Injection", 0, 0),
    ("brief contents", 0, 9),
    ("contents", 0, 11),
    ("preface", 0, 19),
    ("acknowledgments", 0, 21),
    ("about this book", 0, 23),
    ("about the authors", 0, 27),
    ("about the cover illustration", 0, 28),
    ("Part 1: Putting Dependency Injection", 0, 31),
    ("1 The basics of Dependency Injection: What, why, and how", 1, 33),
    ("1.1\tWriting maintainable code", 2, 35),
    ("1.1.1\tCommon myths about DI", 3, 35),
    ("1.1.2\tUnderstanding the purpose of DI", 3, 38),
    ("1.2\tA simple example: Hello DI!", 2, 44),
    ("1.2.1\tHello DI! code", 3, 45),
    ("1.2.2\tBenefits of DI", 3, 47),
    ("1.3\tWhat to inject and what not to inject", 2, 54),
    ("1.3.1\tStable Dependencies", 3, 56),
    ("1.3.2\tVolatile Dependencies", 3, 56),
    ("1.4\tDI scope", 2, 57),
    ("1.4.1\tObject Composition", 3, 59),
    ("1.4.2\tObject Lifetime", 3, 60),
    ("1.4.3\tInterception", 3, 60),
    ("1.4.4\tDI in three dimensions", 3, 61),
    ("1.5\tConclusion", 2, 62),
    ("2 Writing tightly coupled code", 1, 64),
    ("2.1\tBuilding a tightly coupled application", 2, 65),
    ("2.1.1\tMeet Mary Rowan", 3, 65),
    ("2.1.2\tCreating the data layer", 3, 66),
    ("2.1.3\tCreating the domain layer", 3, 69),
    ("2.1.4\tCreating the UI layer", 3, 72),
    ("2.2\tEvaluating the tightly coupled application", 2, 74),
    ("2.2.1\tEvaluating the dependency graph", 3, 74),
    ("2.2.2\tEvaluating composability", 3, 75),
    ("2.3\tAnalysis of missing composability", 2, 77),
    ("2.3.1\tDependency graph analysis", 3, 77),
    ("2.3.2\tData access interface analysis", 3, 78),
    ("2.3.3\tMiscellaneous other issues", 3, 80),
    ("2.4\tConclusion", 2, 80),
    ("3 Writing loosely coupled code", 1, 82),
    ("3.1\tRebuilding the e-commerce application", 2, 83),
    ("3.1.1\tBuilding a more maintainable UI", 3, 86),
    ("3.1.2\tBuilding an independent domain model", 3, 91),
    ("3.1.3\tBuilding a new data access layer", 3, 100),
    ("3.1.4\tImplementing an ASP.NET Core–specific IUserContext Adapter", 3, 101),
    ("3.1.5\tComposing the application in the Composition Root", 3, 103),
    ("3.2\tAnalyzing the loosely coupled implementation", 2, 104),
    ("3.2.1\tUnderstanding the interaction between components", 3, 104),
    ("3.2.2\tAnalyzing the new dependency graph", 3, 105),
    ("Part 2: Catalog", 0, 111),
    ("4 DI patterns", 1, 113),
    ("4.1\tComposition Root", 2, 115),
    ("4.1.1\tHow Composition Root works", 3, 117),
    ("4.1.2\tUsing a DI Container in a Composition Root", 3, 118),
    ("4.1.3\tExample: Implementing a Composition Root using Pure DI", 3, 119),
    ("4.1.4\tThe apparent dependency explosion", 3, 122),
    ("4.2\tConstructor Injection", 2, 125),
    ("4.2.1\tHow Constructor Injection works", 3, 125),
    ("4.2.2\tWhen to use Constructor Injection", 3, 127),
    ("4.2.3\tKnown use of Constructor Injection", 3, 129),
    ("4.2.4\tExample: Adding currency conversions to the featured products", 3, 130),
    ("4.2.5\tWrap-up", 3, 132),
    ("4.3\tMethod Injection", 2, 134),
    ("4.3.1\tHow Method Injection works", 3, 134),
    ("4.3.2\tWhen to use Method Injection", 3, 135),
    ("4.3.3\tKnown use of Method Injection", 3, 141),
    ("4.3.4\tExample: Adding currency conversions to the Product Entity", 3, 142),
    ("4.4\tProperty Injection", 2, 144),
    ("4.4.1\tHow Property Injection works", 3, 144),
    ("4.4.2\tWhen to use Property Injection", 3, 145),
    ("4.4.3\tKnown uses of Property Injection", 3, 148),
    ("4.4.4\tExample: Property Injection as an extensibility model of a reusable library", 3, 148),
    ("4.5\tChoosing which pattern to use", 2, 150),
    ("5 DI anti-patterns", 1, 154),
    ("5.1\tControl Freak", 2, 157),
    ("5.1.1\tExample: Control Freak through newing up Dependencies", 3, 158),
    ("5.1.2\tExample: Control Freak through factories", 3, 159),
    ("5.1.3\tExample: Control Freak through overloaded constructors", 3, 164),
    ("5.1.4\tAnalysis of Control Freak", 3, 165),
    ("5.2\tService Locator", 2, 168),
    ("5.2.1\tExample: ProductService using a Service Locator", 3, 170),
    ("5.2.2\tAnalysis of Service Locator", 3, 172),
    ("5.3\tAmbient Context", 2, 176),
    ("5.3.1\tExample: Accessing time through Ambient Context", 3, 177),
    ("5.3.2\tExample: Logging through Ambient Context", 3, 179),
    ("5.3.3\tAnalysis of Ambient Context", 3, 180),
    ("5.4\tConstrained Construction", 2, 184),
    ("5.4.1\tExample: Late binding a ProductRepository", 3, 184),
    ("5.4.2\tAnalysis of Constrained Construction", 3, 186),
    ("6 Code smells", 1, 193),
    ("6.1\tDealing with the Constructor Over-injection code smell", 2, 194),
    ("6.1.1\tRecognizing Constructor Over-injection", 3, 195),
    ("6.1.2\tRefactoring from Constructor Over-injection to Facade Services", 3, 198),
    ("6.1.3\tRefactoring from Constructor Over-injection to domain events", 3, 203),
    ("6.2\tAbuse of Abstract Factories", 2, 210),
    ("6.2.1\tAbusing Abstract Factories to overcome lifetime problems", 3, 210),
    ("6.2.2\tAbusing Abstract Factories to select Dependencies based on runtime data", 3, 217),
    ("6.3\tFixing cyclic Dependencies", 2, 224),
    ("6.3.1\tExample: Dependency cycle caused by an SRP violation", 3, 225),
    ("6.3.2\tAnalysis of Mary's Dependency cycle", 3, 229),
    ("6.3.3\tRefactoring from SRP violations to resolve the Dependency cycle", 3, 230),
    ("6.3.4\tCommon strategies for breaking Dependency cycles", 3, 234),
    ("6.3.5\tLast resort: Breaking the cycle with Property Injection", 3, 234),
    ("Part 3: Pure DI", 0, 239),
    ("7 Application composition", 1, 241),
    ("7.1\tComposing console applications", 2, 243),
    ("7.1.1\tExample: Updating currencies using the UpdateCurrency program", 3, 244),
    ("7.1.2\tBuilding the Composition Root of the UpdateCurrency program", 3, 245),
    ("7.1.3\tComposing object graphs in CreateCurrencyParser", 3, 246),
    ("7.1.4\tA closer look at UpdateCurrency's layering", 3, 247),
    ("7.2\tComposing UWP applications", 2, 248),
    ("7.2.1\tUWP composition", 3, 248),
    ("7.2.2\tExample: Wiring up a product-management rich client", 3, 249),
    ("7.2.3\tImplementing the Composition Root in the UWP application", 3, 256),
    ("7.3\tComposing ASP.NET Core MVC applications", 2, 258),
    ("7.3.1\tCreating a custom controller activator", 3, 260),
    ("7.3.2\tConstructing custom middleware components using Pure DI", 3, 263),
    ("8 Object lifetime", 1, 266),
    ("8.1\tManaging Dependency Lifetime", 2, 268),
    ("8.1.1\tIntroducing Lifetime Management", 3, 268),
    ("8.1.2\tManaging lifetime with Pure DI", 3, 272),
    ("8.2\tWorking with disposable Dependencies", 2, 275),
    ("8.2.1\tConsuming disposable Dependencies", 3, 276),
    ("8.2.2\tManaging disposable Dependencies", 3, 280),
    ("8.3\tLifestyle catalog", 2, 285),
    ("8.3.1\tThe Singleton Lifestyle", 3, 286),
    ("8.3.2\tThe Transient Lifestyle", 3, 289),
    ("8.3.3\tThe Scoped Lifestyle", 3, 290),
    ("8.4\tBad Lifestyle choices", 2, 296),
    ("8.4.1\tCaptive Dependencies", 3, 296),
    ("8.4.2\tUsing Leaky Abstractions to leak Lifestyle choices to consumers", 3, 299),
    ("8.4.3\tCausing concurrency bugs by tying instances to the lifetime of a thread", 3, 305),
    ("9 Interception", 1, 311),
    ("9.1\tIntroducing Interception", 2, 313),
    ("9.1.1\tDecorator design pattern", 3, 314),
    ("9.1.2\tExample: Implementing auditing using a Decorator", 3, 317),
    ("9.2\tImplementing Cross-Cutting Concerns", 2, 320),
    ("9.2.1\tIntercepting with a Circuit Breaker", 3, 322),
    ("9.2.2\tReporting exceptions using the Decorator pattern", 3, 327),
    ("9.2.3\tPreventing unauthorized access to sensitive functionality using a Decorator", 3, 328),
    ("10 Aspect-Oriented Programming by design", 1, 331),
    ("10.1\tIntroducing AOP", 2, 332),
    ("10.2\tThe SOLID principles", 2, 335),
    ("10.2.1\tSingle Responsibility Principle (SRP)", 3, 336),
    ("10.2.2\tOpen/Closed Principle (OCP)", 3, 336),
    ("10.2.3\tLiskov Substitution Principle (LSP)", 3, 337),
    ("10.2.4\tInterface Segregation Principle (ISP)", 3, 337),
    ("10.2.5\tDependency Inversion Principle (DIP)", 3, 338),
    ("10.2.6\tSOLID principles and Interception", 3, 338),
    ("10.3\tSOLID as a driver for AOP", 2, 338),
    ("10.3.1\tExample: Implementing product-related features using IProductService", 3, 339),
    ("10.3.2\tAnalysis of IProductService from the perspective of SOLID", 3, 341),
    ("10.3.3\tImproving design by applying SOLID principles", 3, 344),
    ("10.3.4\tAdding more Cross-Cutting Concerns", 3, 357),
    ("10.3.5\tConclusion", 3, 366),
    ("11 Tool-based Aspect-Oriented Programming", 1, 371),
    ("11.1\tDynamic Interception", 2, 372),
    ("11.1.1\tExample: Interception with Castle Dynamic Proxy", 3, 374),
    ("11.1.2\tAnalysis of dynamic Interception", 3, 376),
    ("11.2\tCompile-time weaving", 2, 378),
    ("11.2.1\tExample: Applying a transaction aspect using compile-time weaving", 3, 379),
    ("11.2.2\tAnalysis of compile-time weaving", 3, 381),
    ("Part 4: DI Containers", 0, 387),
    ("12 DI Container introduction", 1, 389),
    ("12.1\tIntroducing DI Containers", 2, 391),
    ("12.1.1\tExploring containers' Resolve API", 3, 391),
    ("12.1.2\tAuto-Wiring", 3, 393),
    ("12.2\tConfiguring DI Containers", 2, 402),
    ("12.2.1\tConfiguring containers with configuration files", 3, 403),
    ("12.2.2\tConfiguring containers using Configuration as Code", 3, 407),
    ("12.2.3\tConfiguring containers by convention using Auto-Registration", 3, 409),
    ("12.2.4\tMixing and matching configuration approaches", 3, 415),
    ("12.3\tWhen to use a DI Container", 2, 415),
    ("12.3.1\tUsing third-party libraries involves costs and risks", 3, 416),
    ("12.3.2\tPure DI gives a shorter feedback cycle", 3, 418),
    ("12.3.3\tThe verdict: When to use a DI Container", 3, 419),
    ("13 The Autofac DI Container", 1, 423),
    ("13.1\tIntroducing Autofac", 2, 424),
    ("13.1.1\tResolving objects", 3, 425),
    ("13.1.2\tConfiguring the ContainerBuilder", 3, 428),
    ("13.2\tManaging lifetime", 2, 434),
    ("13.2.1\tConfiguring instance scopes", 3, 435),
    ("13.2.2\tReleasing components", 3, 436),
    ("13.3\tRegistering difficult APIs", 2, 439),
    ("13.3.1\tConfiguring primitive Dependencies", 3, 439),
    ("13.3.2\tRegistering objects with code blocks", 3, 441),
    ("13.4\tWorking with multiple components", 2, 442),
    ("13.4.1\tSelecting among multiple candidates", 3, 443),
    ("13.4.2\tWiring sequences", 3, 447),
    ("13.4.3\tWiring Decorators", 3, 450),
    ("13.4.4\tWiring Composites", 3, 452),
    ("14 The Simple Injector DI Container", 1, 457),
    ("14.1\tIntroducing Simple Injector", 2, 458),
    ("14.1.1\tResolving objects", 3, 459),
    ("14.1.2\tConfiguring the container", 3, 462),
    ("14.2\tManaging lifetime", 2, 468),
    ("14.2.1\tConfiguring Lifestyles", 3, 469),
    ("14.2.2\tReleasing components", 3, 470),
    ("14.2.3\tAmbient scopes", 3, 473),
    ("14.2.4\tDiagnosing the container for common lifetime problems", 3, 474),
    ("14.3\tRegistering difficult APIs", 2, 477),
    ("14.3.1\tConfiguring primitive Dependencies", 3, 478),
    ("14.3.2\tExtracting primitive Dependencies to Parameter Objects", 3, 479),
    ("14.3.3\tRegistering objects with code blocks", 3, 480),
    ("14.4\tWorking with multiple components", 2, 481),
    ("14.4.1\tSelecting among multiple candidates", 3, 482),
    ("14.4.2\tWiring sequences", 3, 484),
    ("14.4.3\tWiring Decorators", 3, 487),
    ("14.4.4\tWiring Composites", 3, 489),
    ("14.4.5\tSequences are streams", 3, 492),
    ("15 The Microsoft.Extensions.DependencyInjection DI Container", 1, 496),
    ("15.1\tIntroducing Microsoft.Extensions.DependencyInjection", 2, 497),
    ("15.1.1\tResolving objects", 3, 498),
    ("15.1.2\tConfiguring the ServiceCollection", 3, 501),
    ("15.2\tManaging lifetime", 2, 506),
    ("15.2.1\tConfiguring Lifestyles", 3, 507),
    ("15.2.2\tReleasing components", 3, 507),
    ("15.3\tRegistering difficult APIs", 2, 510),
    ("15.3.1\tConfiguring primitive Dependencies", 3, 510),
    ("15.3.2\tExtracting primitive Dependencies to Parameter Objects", 3, 511),
    ("15.3.3\tRegistering objects with code blocks", 3, 512),
    ("15.4\tWorking with multiple components", 2, 513),
    ("15.4.1\tSelecting among multiple candidates", 3, 513),
    ("15.4.2\tWiring sequences", 3, 516),
    ("15.4.3\tWiring Decorators", 3, 519),
    ("15.4.4\tWiring Composites", 3, 522),
    ("glossary", 1, 529),
    ("resources", 1, 533),
    ("index", 1, 537),
]

def create_di_toc_test_pdf(output_path: str):
    """Create a test PDF with the exact DI book TOC structure."""
    # Find max page number
    max_page = max(page for _, _, page in TOC_ENTRIES)
    total_pages = max_page + 1

    c = canvas.Canvas(output_path, pagesize=letter)
    width, height = letter

    # Group entries by page for bookmark placement
    entries_by_page = {}
    for i, (title, level, page) in enumerate(TOC_ENTRIES):
        entries_by_page.setdefault(page, []).append((i, title, level))

    # Generate all pages
    for page_num in range(total_pages):
        c.setFont("Helvetica", 10)
        c.drawString(inch, height - 0.5 * inch, f"Page {page_num + 1}")

        # Add bookmarks for entries on this page
        if page_num in entries_by_page:
            for idx, title, level in entries_by_page[page_num]:
                key = f"bm_{idx}"
                c.bookmarkPage(key, fit="FitH", top=height)

        c.showPage()

    # Add outline entries (must be done after all pages are created)
    for i, (title, level, page) in enumerate(TOC_ENTRIES):
        key = f"bm_{i}"
        c.addOutlineEntry(title, key, level=level)

    c.save()
    print(f"Created: {output_path}")
    print(f"  - {len(TOC_ENTRIES)} outline entries")
    print(f"  - {total_pages} pages")
    print(f"  - Levels: 0-3 (front matter, chapters, sections, subsections)")

if __name__ == "__main__":
    import sys
    output = sys.argv[1] if len(sys.argv) > 1 else "tests/testdata/di_book_toc_test.pdf"
    create_di_toc_test_pdf(output)
