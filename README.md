# ALICE-Agent

Local-first coding agent powered by ALICE 1.58-bit ternary models.

A Rust-native alternative to cloud-based coding agents. Loads a quantized LLM directly in-process — no API calls, no Python, no Ollama. Just a single binary.

## Features

- **Local inference**: Load `.alice` ternary models (~3.6GB) directly in Rust
- **API fallback**: OpenAI and Anthropic APIs when no local model is available
- **Tool system**: bash, file read/write/edit, glob, grep — the essentials
- **MCP support**: Connect to Model Context Protocol servers
- **Permission control**: read-only / workspace-write / full-access
- **Session persistence**: Save and resume conversations
- **Project context**: Automatically loads ALICE.md / CLAUDE.md

## Quick Start

```bash
# Build
cargo build --release --features full

# Install
cargo install --path . --features full

# Run with local model
alice --model path/to/model.alice --tokenizer path/to/tokenizer.json

# Run with API (set ANTHROPIC_API_KEY or OPENAI_API_KEY)
alice --provider anthropic

# One-shot mode
alice -p "fix the bug in src/main.rs"

# Resume previous session
alice --resume
```

## Architecture

```
alice (binary)
├── provider/       — LLM backends (local ternary, OpenAI, Anthropic)
├── tools/          — bash, read_file, write_file, edit_file, glob, grep
├── conversation/   — Turn loop, ChatML formatter, tool call parser
├── mcp/            — MCP client (JSON-RPC over stdio)
├── tui/            — REPL interface
├── permission.rs   — 3-level permission system
└── context.rs      — Project context loader
```

## How It Works

1. Load model + tokenizer at startup
2. User types a prompt
3. Model generates a response (may include `<tool_use>` blocks)
4. Tool calls are parsed, permissions checked, tools executed
5. Results are fed back to the model
6. Loop until the model responds with text only

## Tool Calling Format

The model uses XML tags to signal tool calls:

```
<tool_use>
{"name": "bash", "id": "call_001", "input": {"command": "ls -la"}}
</tool_use>
```

Tool results are provided in a `tool` role message:

```
<tool_result id="call_001">
total 48
-rw-r--r--  1 user  staff  1234 main.rs
</tool_result>
```

## Configuration

Global config at `~/.alice-agent/config.toml`:

```toml
model_path = "/path/to/model.alice"
tokenizer_path = "/path/to/tokenizer.json"
max_tokens = 4096
temperature = 0.3
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key (for `--provider anthropic`) |
| `OPENAI_API_KEY` | OpenAI API key (for `--provider openai`) |
| `OPENAI_BASE_URL` | Custom OpenAI-compatible endpoint |
| `OPENAI_MODEL` | Model name override |
| `ANTHROPIC_MODEL` | Model name override |

## License

`AGPL-3.0 OR LicenseRef-Commercial` — dual-licensed. Pick either.

| Option | Terms | Use it when |
|--------|-------|-------------|
| **AGPL-3.0** | [LICENSE-AGPL](LICENSE-AGPL) — free, no reporting obligation | Your project is itself AGPL-compatible open source, or you are only using it internally |
| **Commercial License** | [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) — paid, removes the copyleft | Closed-source product, proprietary SaaS, edge / firmware distribution, plugin redistribution, or a platform NDA that forbids source disclosure |

AGPL is a strong copyleft: a product, firmware image, or service that links
`alice-agent` and is distributed or served to users must be released under the AGPL
as well. That is intentional for the open ecosystem, and the Commercial
License exists for the cases where it is not something you are able to do.

Commercial licence enquiries: <contact@extoria.co.jp>
