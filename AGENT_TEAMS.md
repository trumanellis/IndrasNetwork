# Agent Teams in Claude Code

## 1. Agent Teams (Experimental, Newest)

True multi-agent coordination where teammates work **independently in parallel** with **direct inter-agent communication**:

```
┌─────────────┐
│  Team Lead   │
└──────┬──────┘
   ┌───┼───┐
┌──▼┐┌─▼─┐┌▼──┐
│TM1││TM2││TM3│   ← independent sessions
└───┘└───┘└───┘
    └──┼──┘
    ┌──▴──┐
    │Tasks│  ← shared task list, atomic claiming
    └─────┘
```

Enable with:
```json
{ "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
```

Best for: parallel code review (security/perf/testing lenses), debugging with competing hypotheses, cross-layer feature work.

## 2. Custom Subagents (`.claude/agents/`)

Define reusable specialists as markdown files:

```yaml
# .claude/agents/security-reviewer.md
---
name: security-reviewer
tools: Read, Grep, Glob
model: sonnet
---
You are a security reviewer focusing on OWASP Top 10...
```

Create interactively with `/agents`. They get auto-invoked based on the `description` field.

## 3. Claude Agent SDK (Programmatic)

Build agent teams in Python/TypeScript with full control:

```python
from claude_agent_sdk import query, ClaudeAgentOptions, AgentDefinition

async for message in query(
    prompt="Review auth module",
    options=ClaudeAgentOptions(
        agents={
            "security": AgentDefinition(
                description="Security specialist",
                tools=["Read", "Grep", "Glob"],
                model="sonnet",
            ),
            "perf": AgentDefinition(
                description="Performance analyst",
                tools=["Read", "Grep", "Bash"],
                model="haiku",
            ),
        }
    )
):
    print(message)
```

Key SDK features:

- **Hooks**: Intercept every tool call for audit/security
- **Sessions**: Multi-turn conversations with context persistence
- **Fork**: Branch exploration without losing original context

## 4. Worktree Isolation

Run agents in isolated git worktrees so they can't conflict:

```python
Agent(prompt="...", isolation="worktree")  # gets its own repo copy
```

## 5. Parallel Background Agents

Launch multiple agents concurrently, get notified on completion:

```python
# In a single message, launch 3 agents with run_in_background=True
Agent(prompt="Write tests", run_in_background=True)
Agent(prompt="Write docs", run_in_background=True)
Agent(prompt="Security scan", run_in_background=True)
```

## 6. Key Patterns

### Parallel Code Review with Competing Perspectives

```text
Create an agent team to review this PR:
- Security reviewer: OWASP, injection vulnerabilities, auth/crypto
- Performance reviewer: algorithmic complexity, memory usage, N+1 queries
- Testing reviewer: edge cases, coverage, mocking strategies

Have them share findings and challenge each other's conclusions.
```

### Research with Hypothesis Testing

Spawn 5 subagents to investigate different hypotheses in parallel. Each searches the codebase and tests their hypothesis independently.

### Smart Model Routing

| Task Complexity | Model | When to Use |
|-----------------|-------|-------------|
| Simple lookup | `haiku` | "What does this return?", "Find definition of X" |
| Standard work | `sonnet` | "Add error handling", "Implement feature" |
| Complex reasoning | `opus` | "Debug race condition", "Refactor architecture" |
