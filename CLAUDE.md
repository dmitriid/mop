# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

MOP is a TUI (Terminal User Interface) application written in Rust for discovering and exploring UPnP services, specifically targeting Plex servers. The application allows users to:
- Search for and discover Plex servers on the network via UPnP
- Browse directory structures on discovered Plex servers
- Retrieve information about exposed media files
- Extract direct URLs to media files for access
- run an external command on Enter when pressed on a file

App also uses a TOML config in ~/.config/mop.toml that lists which app to run on Enter, and whether to auto close when that command is run.

## Rust

Never add dependencies directly to Cargo.toml. Always use `cargo add`.

## Go

Never do things manually iof there's a go tool existing. E.g. never add dependencies manually

## Thorough analysis

When asked to analyse something thoroughly:

- re-read CLAUDE.md, as there could be changes to it meanwhile.
- re-analyse related files and functionality thoroughly.
- write all issues to docs/issues/<YYYY-MM-dd>-<sequence-number>-<issue-name>.md.
- Then re-read the analysis, re-analyse upnp implementation, and find all issues with the previous analysis.
- Write down a new one in the same format. Then re-do analysis from scratch of both upnp and the two issues file, and write a comprehensive final solution.
- Then re-read the comprehensive final solution and implement that. You're allowed to re-do everything from scratch, think outside the box etc.

## Commands

Standard Rust project commands:
- Build: `cargo build`
- Run: `cargo run`
- Test: `cargo test`
- Check: `cargo check`
- Lint: `cargo clippy`
- Format: `cargo fmt`

When checking for compilation errors and warnings, use grep with context lines to capture full error messages:
- `cargo build 2>&1 | grep -A 5 -B 5 "error\|warning"`
- `cargo check 2>&1 | grep -A 5 -B 5 "error\|warning"`

## Dependencies

- `ratatui` (v0.29.0) - Terminal User Interface framework with all widgets enabled
- `upnp-client` (v0.1.11) - UPnP client library for service discovery and communication

## Architecture

Key components (to be expanded as implementation progresses):
- UPnP discovery and communication layer (using `upnp-client`)
- Plex server API integration
- TUI interface for browsing and navigation (using `ratatui`)
- File metadata extraction and URL resolution
- emmorize NEVER EVER CREATE MOCK OR TEST DATA WHEN ASKED TO IMPLEMENT FUNCTIONALITY. RETURNING MOCK/TEST DATA WHEN APP IS RUNNING IS USELESS AND ACTIVELY HARMFUL TO THE USER
