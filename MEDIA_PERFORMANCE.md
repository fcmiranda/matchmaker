# Media Preview Performance Analysis

This document details the performance bottlenecks encountered during the implementation of native media previews (Images, Videos, PDFs) via `ratatui-image` in Matchmaker, and the engineering solutions implemented to resolve them.

## 1. Async Task Starvation (The "Queue" Problem)

### The Problem
When rapidly scrolling through a directory containing many media files, the preview UI would lag significantly. When the user finally stopped scrolling, the preview would take several seconds to update to the current file.

Because `image::open` (for high-res photos) and `ffmpegthumbnailer` are heavy blocking operations, we process them on a background thread using `tokio::task::spawn_blocking`. However, without cancellation logic, holding the "down" arrow over 50 files queued 50 separate, sequential image-decoding tasks. The CPU was forced to decode all 49 skipped files before it could process and render the file the user actually landed on.

### The Solution
We implemented **Cross-Thread Abort Checking**.

Since standard `spawn_blocking` tasks cannot be forcefully cancelled in Rust without risking memory leaks, we cloned the `tokio::sync::watch::Receiver` event channel directly into the background closure. 

```rust
let rx = self.rx.clone();
tokio::task::spawn_blocking(move || {
    // 1. Check before starting heavy work
    if rx.has_changed().unwrap_or(false) { return; }
    
    // ... decode image ...

    // 2. Check again before updating the UI state
    if rx.has_changed().unwrap_or(false) { return; }
    
    // ... apply image ...
});
```
By utilizing the channel's state, the background thread instantly checks if the user has scrolled to a new item. If they have, it aborts immediately. This completely bypasses the queue, ensuring CPU cycles are only spent on the image the user is currently viewing.

---

## 2. Video Seeking Delay

### The Problem
Video previews were taking 1-3 seconds to generate, making the TUI feel unresponsive when browsing movies or screen recordings. 

By default, `ffmpegthumbnailer` calculates and seeks to exactly **10%** of the video's total duration to generate a representative frame. For large or highly-compressed files (like a 2-hour 4K MKV), seeking to the 12-minute mark requires parsing file indexes and keyframes, which is an extremely slow I/O bound operation.

### The Solution
We explicitly bypassed the percentage calculation by injecting the `-t` (time) flag into the underlying system command.

```rust
std::process::Command::new("ffmpegthumbnailer")
    .args(["-i", &path, "-s", "512", "-t", "00:00:01", "-c", "jpeg", "-o", "-"])
```
By explicitly instructing the tool to grab the frame at exactly `00:00:01` (1 second), `ffmpegthumbnailer` doesn't need to read the file's duration metadata or seek deep into the index block. It simply extracts the very beginning of the video stream, transforming a multi-second delay into a near-instantaneous operation.

---

## 3. UI Redraw Starvation (Missing Debounce)

### The Problem
Spawning a background thread for *every* movement event creates unnecessary thread overhead and file handle usage, even with our new cross-thread abort check.

### The Solution
We tied the `PreviewMessage::Media` handler into Matchmaker's existing `debounce_ms` loop. 
If the user holds down an arrow key, Matchmaker waits (typically 30-50ms) before it even *attempts* to dispatch the `PreviewMessage::Media` event. Intermediate steps are entirely dropped by the event loop, ensuring maximum scroll fluidity and zero wasted CPU cycles.
