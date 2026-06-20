---
name: agents-rust
description: Rust-powered autonomous AI agent with filesystem, terminal, web, and edit tools available as an MCP server
compatibility: opencode
---

## What I provide

The **agents-rust** MCP server exposes 15 tools that any opencode agent can use:

| Category | Tools |
|----------|-------|
| Filesystem | read_file, write_file, list_directory, create_directory, move_file, get_file_info, search_files |
| Editing | edit_block (SEARCH/REPLACE with fuzzy matching) |
| Terminal | start_process, read_process_output, interact_with_process, force_terminate |
| LLM | ask_llm (direct LLM access) |
| Web | browser_action (fetch, get_html) |
| System | get_environment_info |

## When to use

Use this MCP server when you need filesystem operations, command execution, or web fetching capabilities that complement the built-in tools.

## How to use

The tools are automatically available once this MCP server is enabled. Call them directly:

- `read_file` for reading files with pagination/tail support
- `edit_block` for surgical SEARCH/REPLACE edits with fuzzy fallback
- `start_process` for running shell commands with optional session management
- `browser_action` for fetching web pages as text or HTML
