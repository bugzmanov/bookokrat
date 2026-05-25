#!/usr/bin/env bash
RGB='/wAA'   # 1x1 red pixel, raw RGB, base64

# Transmit AND display two images (a=T creates a placement, which is what
# Ghostty's d=R iterates over).
printf '\x1b_Ga=T,f=24,t=d,s=1,v=1,i=100,p=100,c=10,r=5;%s\x1b\\' "$RGB"
printf '\n'
printf '\x1b_Ga=T,f=24,t=d,s=1,v=1,i=200,p=200,c=10,r=5;%s\x1b\\' "$RGB"
printf '\n'

# Delete by id range that should cover ONLY id=100.
printf '\x1b_Ga=d,d=R,x=99,y=101\x1b\\'

# Probe both images. With a correct implementation, i=200 still exists.
printf '\x1b_Ga=p,i=100,p=100,c=10,r=5\x1b\\'
printf '\n'
printf '\x1b_Ga=p,i=200,p=200,c=10,r=5\x1b\\'
printf '\n'

