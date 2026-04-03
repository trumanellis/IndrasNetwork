# Design System: SyncEngine — The Gift Cycle

## 1. Visual Theme & Atmosphere

SyncEngine's interface is a dark, atmospheric application built on near-black surfaces with a subtle indigo dot-grid texture. The experience feels like a living system — a metabolic loop visualized through warm gradients, pulsing indicators, and layered translucent panels. Where terminal UIs are austere, this design is organic: color bleeds through borders, nodes glow with attention, and trust relationships shimmer across gradient arcs.

The palette is built around a three-point accent gradient — indigo (`#818cf8`) through purple (`#c084fc`) to pink (`#f472b6`) — which represents the gift cycle itself. This gradient appears in hero text, token cards, tag badges, and the hexagonal cycle visualization. Semantic colors (emerald for success/service, amber for attention/warning, red for errors) anchor the system's functional vocabulary without competing with the accent gradient.

Typography uses Inter for all UI text — clean, neutral, and highly legible at small sizes — with JetBrains Mono reserved for data structures, labels, timestamps, and technical metadata. The hierarchy is achieved through weight (300–800) and size rather than font switching, giving the interface a unified voice that shifts from editorial lightness (hero subtitles at weight 300) to structural density (stage titles at weight 700).

**Key Characteristics:**
- Near-black surfaces (`#09090b`) with subtle indigo dot-grid background (`24px` repeat)
- Three-point accent gradient: indigo → purple → pink for brand moments and emphasis
- Inter (300–800) for all UI text; JetBrains Mono for code, labels, and metadata
- Six stage colors mapping the gift cycle: intention (indigo), attention (amber), service (emerald), blessing (purple), token (pink), renewal (indigo)
- Six member colors for avatar identity: Love (pink), Joy (gold), Peace (blue), Grace (lavender), Hope (sage), Faith (orange)
- 12px base border radius — rounded, approachable, not pill-shaped
- Translucent colored borders and backgrounds for stage-specific panels
- Cubic-bezier easing (`0.16, 1, 0.3, 1`) for interactive transforms
- `prefers-reduced-motion` not explicitly handled — consider adding

## 2. Color Palette & Roles

### Backgrounds
- **bg** (`#09090b`): Primary page background. Near-black with a cool undertone.
- **bg2** (`#111114`): Secondary surface — description panels, data panels, navigation bars.
- **card** (`#151518`): Card surfaces — UI mock panels, elevated containers.
- **hover** (`#1c1c20`): Hover state background for interactive surfaces.

### Borders
- **bd** (`#27272a`): Primary border color — panels, dividers, section separators.
- **b2** (`#1c1c20`): Secondary border — subtle internal separators.

### Text
- **tx** (`#fafafa`): Primary text — headings, strong emphasis, input values.
- **t2** (`#a0a0ab`): Secondary text — body copy, descriptions, card content.
- **mt** (`#52525b`): Muted text — labels, metadata, timestamps, section headers.
- **gh** (`#3f3f46`): Ghost text — decorative dots, divider icons, steward arrows.

### Accent Gradient (Brand)
- **ac** (`#818cf8`): Indigo — primary accent, links, interactive elements, intention stage.
- **ac2** (`#c084fc`): Purple — blessing stage, gradient midpoint, attention values.
- **ac3** (`#f472b6`): Pink — token stage, tag badges, gradient endpoint.
- **Gradient**: `linear-gradient(135deg, var(--ac), var(--ac2), var(--ac3))` — hero text, token card top borders, "glow" buttons, emphasis text.

### Semantic
- **ok** (`#34d399`): Emerald — success, service stage, verified claims, proof submissions.
- **wn** (`#fbbf24`): Amber — attention stage, warning, heat indicators, timer values.
- **er** (`#f87171`): Red — error states, destructive actions.

### Stage Colors (Gift Cycle)
| Stage | Token | Hex | Role |
|-------|-------|-----|------|
| Intention | `--stage-intention` | `#818cf8` | Creating a need |
| Attention | `--stage-attention` | `#fbbf24` | Dwelling / noticing |
| Service | `--stage-service` | `#34d399` | Doing the work |
| Blessing | `--stage-blessing` | `#c084fc` | Validating the work |
| Token | `--stage-token` | `#f472b6` | Crystallized gratitude |
| Renewal | `--stage-renewal` | `#818cf8` | Tagging forward |

### Member Colors (Identity)
| Name | Token | Hex |
|------|-------|-----|
| Love | `--love` | `#ff6b9d` |
| Joy | `--joy` | `#ffd93d` |
| Peace | `--peace` | `#6bcfff` |
| Grace | `--grace` | `#b19cd9` |
| Hope | `--hope` | `#98d8aa` |
| Faith | `--faith` | `#ffb347` |

### Translucent Tints (Stage Panels)
Each stage uses its color at very low opacity for panel backgrounds and borders:
- Background tint: `rgba({color}, 0.03–0.06)`
- Border tint: `rgba({color}, 0.10–0.18)`
- Example: intention panel → `background: rgba(129,140,248,0.06)`, `border: 1px solid rgba(129,140,248,0.15)`

## 3. Typography Rules

### Font Families
- **UI / Body**: `'Inter', -apple-system, system-ui, sans-serif`
- **Code / Labels**: `'JetBrains Mono', monospace`

### Hierarchy

| Role | Font | Size | Weight | Line Height | Letter Spacing | Notes |
|------|------|------|--------|-------------|----------------|-------|
| Hero Title | Inter | `clamp(36px, 5vw, 60px)` | 800 | 1.05 | -0.04em | Gradient text via `background-clip` |
| Section Title | Inter | 28px | 700 | 1.2 | -0.03em | Stage detail headings |
| Card Title | Inter | 15px | 600 | — | — | Intent titles, proof names |
| Body | Inter | 15px | 400 | 1.7 | — | Base body size |
| Description | Inter | 14px | 400 | 1.8 | — | Panel descriptions, narratives |
| Small Body | Inter | 13px | 400 | 1.7 | — | Mock UI content, form inputs |
| Small Label | Inter | 12px | 500 | — | — | Mock labels, token keys |
| Hero Subtitle | Inter | 16px | 300 | 1.8 | — | Light weight for editorial feel |
| Eyebrow | JetBrains Mono | 11px | 400 | — | 0.12em | Uppercase, accent colored |
| Stage Number | JetBrains Mono | 10px | 400 | — | 0.12em | Uppercase, stage colored |
| Section Label | JetBrains Mono | 10px | 500 | — | 0.06–0.10em | Uppercase, muted. Panel headers, node labels |
| Data / Code | JetBrains Mono | 11px | 400 | 1.9 | — | Data panels, struct definitions |
| Micro Label | JetBrains Mono | 9–10px | 500 | — | 0.05–0.15em | Timestamps, metadata, badges |
| Formula | JetBrains Mono | 13px | 400 | 2.0 | — | Centered, muted text with gradient emphasis |

### Principles
- **Weight creates hierarchy**: Inter ranges from 300 (hero subtitles) to 800 (hero titles). Body is 400, labels are 500, titles are 600–700.
- **Mono for structure**: JetBrains Mono appears wherever data, time, or identity is displayed — never for prose.
- **Negative letter-spacing for display**: Headlines use -0.03em to -0.04em for tight, engineered feel. Labels use positive spacing (0.06–0.15em) for openness.
- **Gradient text for emphasis**: Key words use `background: linear-gradient(135deg, var(--ac), var(--ac2), var(--ac3))` with `-webkit-background-clip: text`.

## 4. Component Stylings

### Buttons

**Primary (Accent)**
- Background: `var(--ac)` (`#818cf8`)
- Text: `var(--bg)` (dark)
- Padding: 8px 20px
- Radius: 8px
- Font: Inter, 13px, weight 600

**Outline**
- Background: transparent
- Border: `1px solid var(--ac)`
- Text: `var(--ac)`
- Padding: 8px 20px
- Radius: 8px

**Success**
- Background: `var(--ok)` (`#34d399`)
- Text: `var(--bg)`

**Warm**
- Background: `var(--wn)` (`#fbbf24`)
- Text: `var(--bg)`

**Glow (Premium Action)**
- Background: `linear-gradient(135deg, var(--ac), var(--ac2))`
- Text: white
- Box-shadow: `0 0 20px rgba(129,140,248,0.2)`
- Use: "Verify & Bless" — the most important action in the system

**Tag Action**
- Background: `rgba(244,114,182,0.12)`
- Border: `1px solid rgba(244,114,182,0.25)`
- Text: `var(--ac3)` (pink)
- Padding: 8px 14px
- Font: 12px

### Cards & Containers

**UI Mock Panel**
- Background: `var(--card)` (`#151518`)
- Border: `1px solid var(--bd)`
- Radius: `var(--radius)` (12px)
- Top accent: `1px linear-gradient(90deg, transparent, rgba(129,140,248,0.2), transparent)`
- Title bar: `var(--bg2)` background, `1px solid var(--bd)` bottom border
- Title bar dots: 3x 8px circles in `var(--gh)`

**Description Panel**
- Background: `var(--bg2)` (`#111114`)
- Border: `1px solid var(--bd)`
- Radius: 12px
- Padding: 24px
- Header: JetBrains Mono 10px uppercase, muted, letter-spacing 0.1em

**Data Panel (Code)**
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`
- Radius: 12px
- Header: `var(--bg)` background, JetBrains Mono 10px uppercase, letter-spacing 0.08em
- Body: JetBrains Mono 11px, line-height 1.9, `white-space: pre-wrap`
- Syntax colors: `var(--ac)` for keywords, `var(--ok)` for strings/booleans, `var(--wn)` for values, `var(--gh)` italic for comments

**Token Card**
- Background: `linear-gradient(135deg, rgba(244,114,182,0.04), rgba(129,140,248,0.04))`
- Border: `1px solid rgba(244,114,182,0.12)`
- Radius: 14px
- Padding: 20px
- Top accent: 2px gradient bar (`var(--ac3)` → `var(--ac2)` → `var(--ac)`)
- Token ID: JetBrains Mono 11px, pink
- State badge: JetBrains Mono 10px, weight 500, pill (100px radius), `rgba(244,114,182,0.1)` background
- Rows: flex between, 6px vertical padding, `rgba(255,255,255,0.03)` bottom border

**Narrative Block**
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)` + `3px solid {member-color}` left border
- Radius: 12px
- Padding: 20px 24px
- Text: 14px, `var(--t2)`, line-height 1.8

### Inputs & Forms

**Text Input**
- Background: `var(--bg)` (`#09090b`)
- Border: `1px solid var(--bd)`
- Radius: 8px
- Height: 36px
- Padding: 0 12px
- Font: Inter, 13px
- Text: `var(--tx)` for values, `var(--t2)` for placeholders

**Textarea**
- Same as input but height: 80px, padding: 10px 12px
- Text: `var(--t2)`, line-height 1.5

**Label**
- Font: Inter, 12px, weight 500
- Color: `var(--t2)`
- Margin-bottom: 4px

### Status Indicators

**Attention Timer**
- Background: `rgba(251,191,36,0.04)`
- Border: `1px solid rgba(251,191,36,0.15)`
- Radius: 10px
- Padding: 14px 18px
- Pulse dot: 10px circle, `var(--wn)`, animated glow
- Label: JetBrains Mono 11px, amber
- Value: JetBrains Mono 18px, weight 600, amber

**Heat Indicator**
- 8px circle dot with amber color at varying opacity (0.2–0.7 based on heat value)
- Label: JetBrains Mono 10px, amber or ghost depending on heat level

**Proof Card**
- Background: `rgba(52,211,153,0.03)`
- Border: `1px solid rgba(52,211,153,0.12)`
- Radius: 10px
- Padding: 12px 16px
- Thumbnail: 44px square, 8px radius, `var(--bg)` background, centered emoji
- Name: 13px, weight 600, primary text
- Meta: JetBrains Mono 10px, muted

**Tagged Token Badge**
- Background: `linear-gradient(135deg, rgba(244,114,182,0.06), rgba(129,140,248,0.06))`
- Border: `1px solid rgba(244,114,182,0.18)` with shimmer animation
- Radius: 10px
- Padding: 6px 14px
- Font: JetBrains Mono 10px
- Label: `var(--ac3)` (pink), weight 500
- Value: `var(--t2)`

**Participation Badges**
- Pill shape: `border-radius: 100px`
- Padding: 3px 10px
- Font: JetBrains Mono 9px
- Tinted background + border matching semantic color (emerald for verified, purple for humanness)

### Avatars

**Member Avatar**
- Size: 28–40px circle
- Background: member color at 15% opacity
- Text: member color, 10–14px, weight 700 (single letter)
- Outer ring (optional): `2px solid currentColor` at 30% opacity, 3px offset

**Blessing Flow**
- Two avatars connected by animated arrow
- Arrow: `var(--ac2)`, 18px, pulsing scale animation
- Center: attention time in JetBrains Mono 18px, purple
- Label: JetBrains Mono 11px, `var(--ac2)`

### Actor Tags (Inline)
- Padding: 1px 6px
- Radius: 4px
- Font: 13px, weight 600
- Background/text: member color at 10% / full member color
- Example: `.actor-zephyr { background: rgba(107,207,255,0.1); color: var(--peace) }`

### Dividers
- Flex row, centered, gap 12px
- Line: `flex: 1`, 1px height, `linear-gradient(to right, var(--bd), transparent)`
- Label: JetBrains Mono 10px uppercase, letter-spacing 0.1em, muted

### Formula Block
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`
- Radius: 12px
- Padding: 20px 24px
- Font: JetBrains Mono 13px, centered, line-height 2
- Emphasis: gradient text (pink → purple) via `background-clip`
- Dim: `var(--gh)` for operators

## 5. Layout Principles

### Spacing System
- Base body font: 15px
- Component gaps: 6px, 8px, 10px, 12px, 14px, 16px
- Section gaps: 24px (stage body grid), 32px (between sections)
- Major breaks: 40px, 48px, 60px, 64px, 80px
- Hero padding: 100px top, 60px bottom

### Grid & Container
- Page max-width: 900px, centered, padding 0 32px
- Stage body: 2-column grid, gap 24px (`grid-template-columns: 1fr 1fr`)
- Token comparison: 2-column or 3-column grid, gap 12–16px
- Metabolism cards: 3-column grid, gap 16px
- Full-width spans: `grid-column: 1 / -1` for narratives and realm feed mocks

### Whitespace Philosophy
- **Narrative breathing room**: Narrative blocks get 24px bottom margin. Stage details get 80px bottom margin. The generous vertical rhythm gives each stage room to land.
- **Dense data, open prose**: Data panels and token cards are compact (tight padding, small fonts). Prose panels use 24px padding and relaxed line heights (1.7–1.8).
- **Visual separation via dividers**: Gradient-fading horizontal lines with centered uppercase labels mark major section transitions.

### Border Radius Scale
| Value | Use |
|-------|-----|
| 4px | Actor tags, inline elements |
| 8px | Buttons, inputs, internal containers, thumbnails |
| 10px | Proof cards, attention timers, tag badges |
| 12px | `var(--radius)` — primary cards, panels, data panels |
| 14px | Token cards, blessing visuals |
| 16px | Cycle node orbs |
| 50% | Avatars, status dots, heat indicators |
| 100px | Pill badges, state indicators |

## 6. Depth & Elevation

| Level | Treatment | Use |
|-------|-----------|-----|
| Base (Level 0) | `var(--bg)` (`#09090b`) | Page background, inputs |
| Surface (Level 1) | `var(--bg2)` (`#111114`) + `1px solid var(--bd)` | Description panels, data panels, nav bars |
| Card (Level 2) | `var(--card)` (`#151518`) + `1px solid var(--bd)` | UI mock panels |
| Hover (Level 3) | `var(--hover)` (`#1c1c20`) | Interactive hover states |
| Glow | `box-shadow: 0 0 20–30px {color}` | Active cycle nodes, glow buttons |
| Gradient accent | `1–2px linear-gradient` top border | UI mocks (1px), token cards (2px) |

**Depth Philosophy**: Depth is created through four carefully spaced background values separated by only ~10 luminance points each. No drop shadows for structural elevation — the only shadows are colored glows on active/interactive elements. The dot-grid background pattern (`rgba(129,140,248,0.05)` at 24px intervals) provides subtle spatial texture without competing with content.

### Translucent Layering
Stage-specific panels layer translucent color over the base backgrounds:
- Token panels: pink/indigo gradient at 4% over bg
- Blessing panels: purple at 3%
- Proof cards: emerald at 3%
- Attention timers: amber at 4%

This creates chromatic depth — panels "belong" to their stage through color temperature rather than structural elevation.

## 7. Interaction & Motion

### Transitions
- **Color/opacity**: 150ms (hover states, text color changes)
- **Transform**: 400ms `cubic-bezier(0.16, 1, 0.3, 1)` (scale on cycle nodes, interactive elements)
- **General**: 300ms for color transitions on labels

### Animations

| Name | Duration | Timing | Use |
|------|----------|--------|-----|
| `arrow-drift` | 3s | ease-in-out infinite | Cycle ring connecting arcs — opacity pulses 0.3→0.8 |
| `attn-beat` | 2s | ease-in-out infinite | Attention pulse dot — box-shadow glow expands and fades |
| `blessing-pulse` | 2s | ease-in-out infinite | Blessing arrow — opacity 0.4→1.0 with scale 1→1.2 |
| `tagged-shimmer` | 4s | ease-in-out infinite | Tag badge border — opacity pulses 0.18→0.35 |
| `fadeIn` | — | — | Stage detail entrance — translateY(12px) → 0 with opacity |
| `spin` | — | — | Loading states (if needed) |

### Hover States
- **Cycle nodes**: `scale(1.08)`, active state `scale(1.12)` with colored `box-shadow: 0 0 30px`
- **Navigation links**: implicit via cursor and color shift

### Interactive Patterns
- **Cycle ring**: Click a node → scroll to stage detail, update active glow
- **Stage detail panels**: Static display with narrative context — no inline interactivity
- **Mock UIs**: Presentational only (buttons are `cursor: default`)

## 8. Responsive Behavior

### Breakpoints
| Name | Width | Key Changes |
|------|-------|-------------|
| Mobile | `< 768px` | Single-column grids, cycle ring hidden, hero title 32px, padding 20px |
| Desktop | `>= 768px` | Full 2-column stage body, cycle ring visible, hero title clamp up to 60px |

### Collapsing Strategy
- **Cycle ring**: Hidden entirely on mobile (`display: none`) — too complex for small screens
- **Stage body**: `grid-template-columns: 1fr 1fr` → `1fr` on mobile
- **Token grids**: 2–3 column → stack naturally (should add `1fr` fallback)
- **Hero title**: `clamp(36px, 5vw, 60px)` provides fluid scaling
- **Container padding**: 32px → 20px on mobile

### Touch Targets
- Buttons: 8px 20px padding, minimum ~36px height
- Cycle nodes: 120px × 120px hit area
- Mock inputs: 36px height

## 9. Agent Prompt Guide

### Quick Color Reference
| Name | Hex | Role |
|------|-----|------|
| Page bg | `#09090b` | Primary background |
| Surface | `#111114` | Panels, secondary bg |
| Card | `#151518` | Mock panels, elevated |
| Border | `#27272a` | All borders |
| Text | `#fafafa` | Primary text |
| Text secondary | `#a0a0ab` | Body, descriptions |
| Text muted | `#52525b` | Labels, metadata |
| Ghost | `#3f3f46` | Decorative, dots |
| Indigo | `#818cf8` | Primary accent |
| Purple | `#c084fc` | Blessing, gradient mid |
| Pink | `#f472b6` | Token, gradient end |
| Emerald | `#34d399` | Success, service |
| Amber | `#fbbf24` | Attention, warning |
| Red | `#f87171` | Error |

### Quick Font Reference
| Role | Family | Fallbacks |
|------|--------|-----------|
| UI / Body | Inter | -apple-system, system-ui, sans-serif |
| Code / Labels | JetBrains Mono | monospace |

### Quick Spacing Reference
- Container: max-width 900px, padding 32px (20px mobile)
- Grids: 2-col at 24px gap (stage body), 3-col at 16px gap (metabolism)
- Section margin: 80px (stage details), 40px (subsections), 24px (narratives)
- Component padding: 24px (panels), 20px (token cards, ui-content), 14–18px (timers, tags)

### Example Component Prompts
- "Create a stage detail header on `#09090b`. Left: 48px icon square with `14px` radius, `2px solid #818cf8` border, `rgba(129,140,248,0.06)` fill, centered emoji. Right: stage number in JetBrains Mono 10px uppercase `#818cf8`, title in Inter 28px weight 700, `-0.03em` spacing. Arrow text in JetBrains Mono 12px `#52525b`. Bottom border `1px solid #27272a`."
- "Build a token card: `linear-gradient(135deg, rgba(244,114,182,0.04), rgba(129,140,248,0.04))` background, `1px solid rgba(244,114,182,0.12)` border, 14px radius, 20px padding. Top: 2px gradient stripe `#f472b6 → #c084fc → #818cf8`. Token ID in JetBrains Mono 11px pink. Rows: Inter 12px muted key, JetBrains Mono 12px secondary value, separated by `rgba(255,255,255,0.03)` lines."
- "Create an attention timer: `rgba(251,191,36,0.04)` background, `1px solid rgba(251,191,36,0.15)` border, 10px radius, 14px 18px padding. Left: 10px amber circle with pulsing box-shadow. Center: JetBrains Mono 11px amber label. Right: JetBrains Mono 18px weight 600 amber time value."
- "Design a tagged token badge: `linear-gradient(135deg, rgba(244,114,182,0.06), rgba(129,140,248,0.06))` background, `1px solid rgba(244,114,182,0.18)` border with shimmer animation, 10px radius, 6px 14px padding. All JetBrains Mono 10px. Tag icon 13px, label in pink weight 500, value in secondary text."
- "Build a member avatar: 36px circle, `rgba({member-color}, 0.15)` background, member-color text, 13px weight 700 single letter. Optional outer ring: 3px offset circle, `2px solid currentColor` at 30% opacity."

### Do's and Don'ts

**Do:**
- Use the indigo→purple→pink gradient for brand emphasis and key interactive moments
- Apply stage colors at low opacity (3–6%) for panel backgrounds to create chromatic depth
- Use Inter weight range (300–800) to create hierarchy within a single font family
- Reserve JetBrains Mono for anything structural: data, timestamps, IDs, labels
- Use `cubic-bezier(0.16, 1, 0.3, 1)` for physical-feeling transform animations
- Apply member colors consistently for avatar identity across all views

**Don't:**
- Use the accent gradient on large surfaces — reserve it for text highlights, thin borders, and button fills
- Mix Inter and JetBrains Mono within the same semantic role (e.g., don't use mono for body prose)
- Use drop shadows for structural elevation — depth comes from background color steps
- Add border radius values not in the established scale (4, 8, 10, 12, 14, 16, 50%, 100px)
- Use raw semantic colors (emerald, amber, red) for decoration — they carry functional meaning
- Apply stage colors outside their semantic context (e.g., don't use service-emerald for a non-success element)
