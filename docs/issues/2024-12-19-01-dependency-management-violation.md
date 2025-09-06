# Dependency Management Violation

**Date:** 2024-12-19
**Issue #:** 01
**Type:** Process Violation

## Problem

The CLAUDE.md file explicitly states: "Never add dependencies directly to Cargo.toml. Always use `cargo add`." However, during the implementation of the settings dialog with ratatui_input, dependencies were manually added to Cargo.toml instead of using the proper `cargo add` command.

## Impact

- Violation of established project guidelines
- Potential for dependency version conflicts
- Inconsistent dependency management approach
- Compilation errors due to incorrect dependency specification

## Root Cause

The developer (Claude) failed to follow the established process outlined in CLAUDE.md for dependency management.

## Solution

1. Remove manually added dependencies from Cargo.toml
2. Use `cargo add` command to properly add dependencies
3. Follow the established process for all future dependency additions

## Prevention

- Always re-read CLAUDE.md before making changes
- Use `cargo add` for all dependency additions
- Verify dependency management process compliance
