# docsite_pdf.tape
# Capture PDF view with Rosepine Moon theme for docs slideshow

file Build a Large Language Model (From Sc....pdf

# Let the PDF render
wait 2000

# Set Rosepine Moon theme: Space+t opens settings
key space
wait 200
key t
wait 500

# Navigate to Rosepine Moon (index 4): j j j j
key j
key j
key j
key j
wait 200
return
wait 300
escape
wait 500

# Go to PDF page 31: Space+g, switch to PDF mode, type 31
key space
wait 200
key g
wait 300
key p
wait 200
key 3
key 1
wait 200
return
wait 1000

# Zoom out 4x to ~82%
key -
wait 300
key -
wait 300
key -
wait 300
key -
wait 500

# Expand TOC: Tab to nav panel, L to expand all, Tab back
tab
wait 300
key L
wait 300
tab
wait 500

screenshot pdf_view

# Quit
key q
