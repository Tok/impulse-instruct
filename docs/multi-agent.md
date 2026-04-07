# Multi-Agent Jamming

Impulse Instruct supports multiple LLM agents collaborating as a virtual
production team.  Each agent has its own persona, model, scope, and creative
perspective.  They take turns in a round-robin jam loop, evolving different
parts of the track simultaneously.

---

## Concept

A single agent controlling everything works well for quick sessions.  But for
longer jams or more structured production, splitting responsibilities across
specialised agents produces more interesting results:

- A **conductor** oversees the sequencer and global structure
- A **bass specialist** focuses on the 303 filter, pattern, and acid character
- A **drum agent** shapes the 808/909 kits independently
- An **FX engineer** sculpts the effects chain, reverb tails, delay feedback

Each agent only sees and controls the modules it's wired to.  This prevents
one agent from overwriting another's work and lets each develop its part of
the track with focused attention.

---

## Recommended Configurations

The startup wizard offers presets, but you can build any combination.  The
general recommendation is **one Gemma 4 E4B as the primary model** plus
**0-4 Bonsai 8B agents** as lightweight specialists, depending on available
VRAM.

| Setup | Models | VRAM | Best for |
|-------|--------|------|----------|
| **Solo** | 1x Gemma | ~6 GB | Quick sessions, full control from one agent |
| **Duo** | 2x Gemma (shared) | ~6 GB | Bass + drums/FX split |
| **Swarm** | 1x Gemma + 3x Bonsai | ~8 GB | Lead producer + 3 specialists |
| **Band** | 1x Gemma + 4x Bonsai | ~8 GB | Conductor + bass/drums/keys/FX |
| **Voices** | 1x Gemma + 4x Bonsai | ~8 GB | One agent per voice group |
| **Lite** | 1x Bonsai | ~2 GB | Minimal VRAM, fast responses |

Gemma 4 E4B is the strongest model for musical understanding and JSON accuracy
(passes all 39 integration tests).  Bonsai 8B is much smaller and faster but
less capable — it works well for focused tasks where the scope is narrow
(e.g. "just handle the bass filter").

Agents sharing the same model share a single llama-server process (ref-counted
in `LlamaServerPool`), so two Gemma agents don't cost 12 GB — they share the
same 6 GB server.

---

## How It Works

### Agent cards

Each agent appears as a compact card in the Global rack zone.  The card shows:

- **Persona name** — editable, shown in the log when the agent responds
- **Inference indicator** — pulsing dot while the agent is thinking, tok/s rate
- **Model selector** — dropdown to pick a GGUF model (or inherit the default)
- **VRAM estimate** — approximate GPU memory for the selected model
- **Temperature** — per-agent sampling temperature
- **Jam bars** — how many bars to wait between jam cycles (0 = continuous)
- **Conversation mode** — Off / Producer / DJ / MC
- **Style** — genre/style from the built-in catalog
- **User instructions** — persistent text injected into the system prompt
- **Scope** — read-only display of which modules the agent controls (derived
  from control cables)

### Control cables

Agent scope is determined by **control cables** on the back panel (press Tab
to flip the rack).  Each agent has a Control output port; modules have Control
input ports.  Drawing a cable from an agent to a module adds that module to the
agent's scope.

- An agent with **no control cables** controls everything (unrestricted)
- An agent wired to specific modules **only** controls those modules
- Removing a cable immediately restricts the agent's scope
- The system prompt tells the agent what it controls, and `apply_llm_update()`
  enforces the scope — an agent cannot modify parameters outside its wiring

### Round-robin scheduling

When the HEAT slider is above 0%, agents take turns in round-robin:

1. An agent completes its inference and sends `[jam_cycle_done]`
2. The UI picks the next enabled agent from the list
3. After a delay based on that agent's `jam_bars` setting, the next inference
   fires with `agent_id` set to the selected agent
4. The agent generates a mutation scoped to its wired modules
5. The result is applied and the cycle continues

Each agent's card shows a pulsing dot while it's inferring and a cycle count
(`#N`) when idle, so you can see the round-robin progressing through the team.

### User prompts

When you type a prompt in the LLM console, it's broadcast to **all enabled
agents**.  Each agent interprets the prompt within its own scope — the bass
agent adjusts the bass, the drum agent adjusts drums, etc.  This means a
prompt like "make it more acid" triggers coordinated changes across the whole
team.

---

## Setup

### First launch (startup wizard)

On first launch, the startup wizard detects your GPU and available VRAM, then
offers preset configurations.  If you have an existing session, "Resume last
session" is the default.

### Manual setup

1. Open the rack (the main UI area)
2. Click **[+ ADD]** in the Global zone rail
3. Select **LLM Agent** to add a new agent
4. Configure the agent card: set persona, model, style, instructions
5. Press **Tab** to flip to the back panel
6. Drag a control cable from the agent's CTL output to the modules it should
   control
7. Flip back to the front panel — the agent's SCOPE line shows what it controls

### Adding / removing agents at runtime

Agents can be added or removed at any time.  The round-robin adjusts
automatically.  If only one agent remains, its scope is cleared (it controls
everything).

Agents can also spawn or dismiss themselves via JSON actions when
`agent_autonomy` is enabled in settings:

```json
{ "settings": { "spawn_agent": { "persona": "FX", "scope": ["fx"], "model": "bonsai" } } }
{ "settings": { "dismiss": true } }
```

---

## Tips

- **Start with Solo**, get a feel for the sound, then add specialists
- **Lock parameters** you care about before adding agents — agents respect locks
- **Use narrow scopes** for Bonsai agents — they work best with focused tasks
- **Set different jam_bars** per agent — stagger their cycles for variety
- **Watch the log** — each agent's persona name appears before its response
- **Lower heat for specialists** — a bass agent at heat 20% makes subtle filter
  tweaks; at heat 80% it rewrites the whole pattern
