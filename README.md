# What is it

A purely vibe-coded TUI to browse UPnP services on the local network. Main use-case: using in [Omarchy](https://omarchy.org/).

For some reason both Plex and VLC struggle with plaing media from my home Plex server. MPV plays it just fine when provided a direct URL. So my biggest gripe was: not good way of quickly navigating to what I want and invoking `mpv <direct url to file on plex>`.

So I vibecoded this.

By default this launches mpv and closes.

<img width="806" height="606" alt="screenshot-2025-09-11_19-23-36" src="https://github.com/user-attachments/assets/e89936de-f141-499e-a277-126c11c4d351" />

<img width="806" height="606" alt="screenshot-2025-09-11_19-22-45" src="https://github.com/user-attachments/assets/56b574fb-d4d0-4e4a-bdf2-459645b48571" />


# How to get it

No binaries, sorry. You'll need to clone this repo and build it.

**Requires Rust**

```
> git clone https://github.com/dmitriid/mop.git
> cd mop
> cargo build -r
> ./target/release/mop
```

# To add it as a TUI app in Omarchy

- Invoke system menu (`Compose+Alt+Space`)
- Install -> TUI
- Put the path to your built binary (e.g. `/home/<username>/projects/mop/target/release/mop`)
- Enjoy

# Issues etc.

- I don't know Rust
- I know very little about TUIs
- I know nothing about UPnP

This is 100% vibe-coded without once looking into the code. I am sorry.

But it works on my machine :)
