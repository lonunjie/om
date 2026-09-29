#!/usr/bin/env python3
"""Reference OM fallback-avatar generator.
Fixture/reference only. Never derive from private keys, recovery secrets, phone numbers, or email addresses.
"""
from __future__ import annotations
import argparse, hashlib, html
from pathlib import Path

PALETTE = [
    ("#2457C5", "#FFFFFF"), ("#4338CA", "#FFFFFF"),
    ("#5B35B5", "#FFFFFF"), ("#6D28D9", "#FFFFFF"),
    ("#155E75", "#FFFFFF"), ("#075985", "#FFFFFF"),
    ("#334EAC", "#FFFFFF"), ("#4C3C9D", "#FFFFFF"),
]

def initials(name: str) -> str:
    words=[w for w in name.replace("-"," ").split() if w]
    if not words: return "?"
    if len(words)==1: return words[0][:2].upper()
    return (words[0][0]+words[-1][0]).upper()

def make(seed: str, name: str, size: int=128) -> str:
    h=hashlib.sha256(("om-avatar-v1\0"+seed).encode()).digest()
    bg,fg=PALETTE[h[0] % len(PALETTE)]
    pattern=h[1] % 4
    opacity=.16
    shapes=[]
    if pattern==0:
        for i in range(3):
            x=18+i*38+(h[2+i]%9)
            shapes.append(f'<circle cx="{x}" cy="{24+i*32}" r="22" fill="{fg}" opacity="{opacity}"/>')
    elif pattern==1:
        shapes += [f'<path d="M-10 {30+i*34} L138 {4+i*34}" stroke="{fg}" stroke-width="14" opacity="{opacity}"/>' for i in range(4)]
    elif pattern==2:
        shapes += [f'<rect x="{8+i*30}" y="{12+(i%2)*34}" width="42" height="42" rx="12" fill="none" stroke="{fg}" stroke-width="8" opacity="{opacity}"/>' for i in range(3)]
    else:
        shapes += [f'<path d="M64 {8+i*18} L{118-i*12} 64 L64 {120-i*18} L{10+i*12} 64 Z" fill="none" stroke="{fg}" stroke-width="7" opacity="{opacity}"/>' for i in range(3)]
    txt=html.escape(initials(name))
    label=html.escape(name)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="{size}" height="{size}" role="img" aria-label="Fallback avatar for {label}">'
        f'<rect width="128" height="128" rx="32" fill="{bg}"/>' + ''.join(shapes) +
        f'<text x="64" y="76" text-anchor="middle" font-family="system-ui,-apple-system,sans-serif" font-size="40" font-weight="700" fill="{fg}">{txt}</text>'
        '</svg>'
    )

def main():
    ap=argparse.ArgumentParser(); ap.add_argument('seed'); ap.add_argument('name'); ap.add_argument('-o','--output',type=Path); args=ap.parse_args()
    svg=make(args.seed,args.name)
    if args.output: args.output.write_text(svg)
    else: print(svg)
if __name__=='__main__': main()
