# Bevy UI Carousel

A simple UI carousel built with Bevy 0.17, featuring smooth sliding animations and multiple navigation methods.

## Features

- Multiple pages in a horizontal carousel.
- Keyboard navigation with arrow keys or `A` / `D`.
- Previous and next buttons.
- Mouse drag with snap-to-nearest behavior.
- Smooth cubic easing for slide transitions.
- Responsive layout updates on resize.
- Overflow clipping and pointer picking support.

## How It Works

The demo is a single Bevy app where pages live side by side inside one UI track. Navigation works by animating the track position instead of swapping scenes or rebuilding the UI tree.

It also simulates wraparound behavior by rotating children when moving past the ends, so the carousel keeps feeling continuous even though the page list is small and fixed.

Drag input, button clicks, keyboard navigation, and resize handling all live in `src/main.rs`.

## Run

```bash
cargo run
```

## Limitations

- This is a demo app, not a reusable crate or plugin yet.
- Pages and colors are hardcoded.
- The whole implementation currently lives in a single file.

## Demo

https://www.youtube.com/watch?v=Ra4cqBvcbsk
