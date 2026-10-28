#!/bin/sh
# Regenerates the PNG icons and the social preview image from the SVG sources in public/icons.
# Needs ImageMagick (`magick`) with its built-in SVG renderer; the SVGs are path-only (no fonts), so output is stable.
# Run: sh apps/web/build/make-icons.sh   (output is committed; the build never runs this)
set -eu
cd "$(dirname "$0")/../public"
r() { magick -background none -density 384 "$1" -resize "${2}x${2}" -strip -define png:exclude-chunks=date,time "$3"; }
r icons/icon.svg 192 icons/icon-192.png
r icons/icon.svg 512 icons/icon-512.png
r icons/icon-maskable.svg 512 icons/icon-maskable-512.png
r icons/icon-maskable.svg 180 icons/apple-touch-icon.png   # iOS applies its own rounded mask to a full-bleed square
magick -size 1200x630 xc:'#0F1217' \( -background none -density 384 icons/icon.svg -resize 280x280 \) -gravity west -geometry +140+0 -composite \
  -strip -define png:exclude-chunks=date,time og-image.png
