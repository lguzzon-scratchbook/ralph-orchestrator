# Backends

Ralph supports multiple AI CLI backends. This guide covers setup and selection.

## Supported Backends

| Backend | CLI Tool | Notes |
|---------|----------|-------|
| Claude Code | `claude` | Recommended, primary support |
| Kiro | `kiro` | Amazon/AWS |
| Gemini CLI | `gemini` | Google |
| Codex | `codex` | OpenAI |
| Amp | `amp` | Sourcegraph |
| Copilot CLI | `copilot` | GitHub |
| OpenCode | `opencode` | Community |

## Auto-Detection

Ralph automatically detects installed backends:

```bash
ralph init
# Auto-detects available backend
```

Detection order (first available wins):
1. Claude
2. Kiro
3. Gemini
4. Codex
5. Amp
6. Copilot
7. OpenCode

## Explicit Selection

Override auto-detection:

```bash
# Via CLI
ralph init --backend kiro
ralph run --backend gemini

# Via config
# ralph.yml
cli:
  backend: "claude"
```

## Backend Setup

### Claude Code

The recommended backend with full feature support.

```bash
# Install
npm install -g @anthropic-ai/claude-code

# Authenticate
claude login

# Verify
claude -p "Hello"
```

**Features:**
- Full streaming support
- All hat features
- Memory integration

### Kiro

Amazon/AWS AI assistant.

```bash
# Install
# Visit https://kiro.dev/

# Verify
kiro -p "Hello"
```

**Notes:**
- May require AWS credentials
- Supports streaming

### Gemini CLI

Google's AI CLI.

```bash
# Install
npm install -g @google/gemini-cli

# Configure API key
export GOOGLE_API_KEY=your-key

# Verify
gemini -p "Hello"
```

### Codex

OpenAI's code-focused model.

```bash
# Install
# Visit https://github.com/openai/codex

# Configure
export OPENAI_API_KEY=your-key

# Verify
codex -p "Hello"
```

### Amp

Sourcegraph's AI assistant.

```bash
# Install
# Visit https://github.com/sourcegraph/amp

# Verify
amp -p "Hello"
```

### Copilot CLI

GitHub's AI assistant.

```bash
# Install
npm install -g @github/copilot

# Authenticate
copilot auth login

# Verify
copilot -p "Hello"
```

### OpenCode

Community AI CLI.

```bash
# Install
curl -fsSL https://opencode.ai/install | bash

# Verify
opencode -p "Hello"
```

## Per-Hat Backend Override

Different hats can use different backends:

```yaml
hats:
  planner:
    backend: "claude"  # Use Claude for planning
    triggers: ["task.start"]
    instructions: "Create a plan..."

  coder:
    backend: "kiro"    # Use Kiro for coding
    triggers: ["plan.ready"]
    instructions: "Implement..."
```

## Custom Backends

For unsupported CLIs, use the custom backend:

```yaml
cli:
  backend: "custom"
  custom_command: "my-ai-cli"
  prompt_mode: "arg"  # or "stdin"
```

**Prompt modes:**

| Mode | How Prompt is Passed |
|------|---------------------|
| `arg` | `my-ai-cli -p "prompt"` |
| `stdin` | `echo "prompt" \| my-ai-cli` |

## Backend Comparison

| Feature | Claude | Kiro | Gemini | Codex |
|---------|--------|------|--------|-------|
| Streaming | Yes | Yes | Yes | Yes |
| Tool use | Full | Full | Partial | Partial |
| Context size | Large | Large | Large | Medium |
| Speed | Fast | Fast | Fast | Medium |
| Cost | $$ | $ | $ | $$ |

## Troubleshooting

### Backend Not Found

```
ERROR: No AI agents detected
```

**Solution:**
1. Install a supported backend
2. Ensure it's in your PATH
3. Test directly: `claude -p "test"`

### Authentication Failed

```
ERROR: Authentication required
```

**Solution:**
```bash
# Claude
claude login

# Copilot
copilot auth login

# Gemini - set API key
export GOOGLE_API_KEY=your-key
```

### Wrong Backend Used

```bash
# Force specific backend
ralph run --backend claude

# Or set in config
cli:
  backend: "claude"
```

### Backend Hanging

Some backends need interactive authentication on first run:

```bash
# Run backend directly first
claude -p "test"

# Then use with Ralph
ralph run
```

## Best Practices

1. **Pick one primary backend** — Consistency helps
2. **Test backend directly** — Before using with Ralph
3. **Use per-hat overrides sparingly** — Can complicate debugging
4. **Keep backends updated** — New features, bug fixes

## Next Steps

- Configure [Presets](presets.md) for your workflow
- Learn about [Cost Management](cost-management.md)
- Explore [Writing Prompts](prompts.md)
