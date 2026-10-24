# ALICE-Agent

Local-first coding agent powered by ALICE 1.58-bit ternary models.

A Rust-native alternative to cloud-based coding agents. Loads a quantized LLM directly in-process — no API calls, no Python, no Ollama. Just a single binary.

## Features

- **Local inference**: Load `.alice` ternary models (~3.6GB) directly in Rust
- **API fallback**: OpenAI and Anthropic APIs when no local model is available (with a key set, a configuration error such as a missing model stops with that error; the echo mode for tool testing starts only when no API key is set)
- **Tool system**: bash, file read/write/edit, glob, grep — the essentials
- **MCP support**: Connect to Model Context Protocol servers
- **Permission control**: read-only / workspace-write / full-access
- **Session persistence**: Save and resume conversations
- **Project context**: Loads the project's instruction file from the working directory (`ALICE.md` by default; set `context_files` in `config.toml` to search other names in order)

## Quick Start

```bash
# Build
cargo build --release --features full

# Install
cargo install --path . --features full

# Run with local model
alice --model path/to/model.alice --tokenizer path/to/tokenizer.json

# Run with API (set ANTHROPIC_API_KEY + ANTHROPIC_MODEL, or OPENAI_API_KEY)
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

Global config at `~/.alice-agent/config.toml` (optional: without the file the defaults are used; a file that cannot be read or parsed stops `alice` with an error naming the file and the reason):

```toml
model_path = "/path/to/model.alice"
tokenizer_path = "/path/to/tokenizer.json"
max_tokens = 4096
temperature = 0.3
context_files = ["ALICE.md"]   # project instruction files, searched in order; the first found is read
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Anthropic API key (for `--provider anthropic`, and the fallback when no local model is loaded) |
| `OPENAI_API_KEY` | OpenAI API key (for `--provider openai`, and the fallback when no local model is loaded) |
| `OPENAI_BASE_URL` | Custom OpenAI-compatible endpoint |
| `OPENAI_MODEL` | Model name override |
| `ANTHROPIC_MODEL` | Model name (required with `ANTHROPIC_API_KEY`; there is no default) |

## Working on this repository

Files that belong to your own editor or local tools, and are not part of the project, go in your
clone's `.git/info/exclude` rather than in `.gitignore`. That file is never committed, so add the
entries again in each new clone.

## License

`LicenseRef-Proprietary` from this version: the source is published for reading only, and
using, copying, modifying or distributing it needs a written agreement with
the copyright holder. The text is [LICENSE-Proprietary](LICENSE-Proprietary). Versions
published before keep the licence they were published under.

Enquiries: https://extoria.co.jp/
