# Design System: Synchronicity Engine / Indra's Network

## 1. Visual Theme & Atmosphere

The Synchronicity Engine is a nocturnal gift-economy dashboard — a developer-focused control center that visualizes the invisible flow of value between people in a trust network. The entire experience is built on an almost-pure-black canvas (`#09090b`) where content surfaces float within barely-visible zinc borders, creating the sensation of a deep-space interface rather than a traditional application. It is a UI that rewards attention: the more you look, the more you see.

The visual language is grounded in the metaphysics of its subject matter. Indra's Network is a Buddhist concept of infinite interconnected jewels, each reflecting every other. The design honors this with a dot-grid texture covering every page — tiny indigo pinpoints on void black, suggesting a field of stars or a crystalline lattice. The six gift-cycle stages (Intention → Attention → Service → Blessing → Token → Renewal) each carry a distinct color, and those colors tint panel backgrounds at 3–6% opacity, creating a subtle chromatic atmosphere that shifts as the user moves through the cycle.

The signature accent is a three-stop gradient — indigo `#818cf8` through purple `#c084fc` to pink `#f472b6` — that runs from cool to warm like a prism bending light through violet. It appears on gradient-text hero headings, glow buttons, and token-value displays. Everywhere else, color is disciplined. The neutral scale is strictly zinc-based (cool, never warm), and secondary text fades through a precise hierarchy from Pure Light to Ghost Zinc. JetBrains Mono carries the technical voice for all labels, stage numbers, and data keys; Inter carries the human voice for everything else.

**Key Characteristics:**
- Void-black canvas (`#09090b`) with a fixed dot-grid overlay that conveys depth without ornamentation
- Three-stop indigo-purple-pink gradient as the single signature accent — used sparingly for maximum impact
- Six semantic stage colors, each with a strict meaning in the gift-cycle domain model
- Cool zinc neutral scale — no warm grays, no beige, no ochre
- Inter for all human-readable content; JetBrains Mono exclusively for labels, keys, and code
- Depth through background layering and 1px zinc borders — not drop shadows
- Generous body line-heights (1.7+) that feel meditative and readable at length
- Tinted panel backgrounds at ultra-low opacity (3–6%) anchor each stage in its color world

## 2. Color Palette & Roles

### Backgrounds
- **Deep Void** (`#09090b`): The primary page background. Not pure black — the faint zinc warmth prevents eye strain during long sessions in dark environments. Every page begins here.
- **Dark Surface** (`#111114`): Secondary background used for panels, sidebars, and the primary UI chrome. One step above Deep Void, it creates a clear structural separation without any harsh contrast.
- **Card Surface** (`#151518`): Card interiors, modal backgrounds, and elevated containers. The workhorse surface for content — distinct from both page and panel backgrounds.
- **Hover Surface** (`#1c1c20`): Used exclusively for interactive hover states and subtle active indicators. Applying this as a background communicates affordance without color.

### Text
- **Pure Light** (`#fafafa`): Primary headings and high-emphasis content. Used at full strength only for the most important text on screen — hero titles, stage names, primary labels.
- **Soft Gray** (`#a0a0ab`): Secondary body text, descriptions, and supporting prose. The workhorse text color — readable, comfortable, clearly subordinate to Pure Light.
- **Muted Zinc** (`#52525b`): De-emphasized metadata, timestamps, and secondary labels. Signals "this is context, not content."
- **Ghost Zinc** (`#3f3f46`): The faintest readable text — used for decorative labels, divider annotations, and UI chrome that should recede entirely.

### Borders
- **Border Primary** (`#27272a`): Standard container borders for panels, cards, and inputs. Visible enough to define boundaries, restrained enough not to create visual noise.
- **Border Subtle** (`#1c1c20`): Faint separators between list items, rows, and nested containers. Communicates structure without weight.

### Accent Gradient (The Signature)
- **Indigo** (`#818cf8`): Primary accent color — links, interactive elements, focus rings, the Intention and Renewal stages. Anchors the identity with a cool, electric quality.
- **Purple** (`#c084fc`): Secondary accent — the Blessing stage, validation states, and the mid-stop of the gradient. Bridges indigo and pink with a mystical warmth.
- **Pink** (`#f472b6`): Tertiary accent — the Token stage, gratitude indicators, and the warm terminus of the gradient. Energetic, celebratory, used sparingly.
- **Signature Gradient**: `linear-gradient(135deg, #818cf8, #c084fc, #f472b6)` — applied as gradient text on hero headings and as the background of glow buttons. The three colors appear together only at highest-emphasis moments.

### Stage Colors (Gift Cycle Semantics)
These colors carry semantic meaning tied to the six stages of the gift cycle. They must not be used interchangeably.

- **Intention** (`#818cf8` — Indigo): The seeding of a gift. Shares its color with the primary accent, anchoring all beginnings in indigo.
- **Attention** (`#fbbf24` — Amber): The sustained focus required to develop a gift. Warm, energetic, attention-demanding by design.
- **Service** (`#34d399` — Emerald): The act of giving. Green signals health, growth, and completion — the color of productive action.
- **Blessing** (`#c084fc` — Purple): The reception and validation of a gift. Purple carries a sense of grace and elevation.
- **Token** (`#f472b6` — Pink): The material or symbolic record of exchange. Pink is celebratory, marking the moment value crystallizes.
- **Renewal** (`#818cf8` — Indigo): The cycle beginning again. Returns to indigo, closing the loop with intention.

### Member Identity Colors
Six named member archetypes each carry a distinct identity color for inline badges and presence indicators:
- **Love** (`#ff6b9d`), **Joy** (`#ffd93d`), **Peace** (`#6bcfff`)
- **Grace** (`#b19cd9`), **Hope** (`#98d8aa`), **Faith** (`#ffb347`)

### Semantic
- **Success** (`#34d399`): Matches the Service stage color — success is an act of service.
- **Warning** (`#fbbf24`): Matches the Attention stage color — warnings demand attention.
- **Error** (`#f87171`): Warm red, distinct from all stage colors, for destructive or failed states.

## 3. Typography Rules

### Font Family
- **Primary**: `'Inter', -apple-system, system-ui, sans-serif` — weights 300 through 800. Inter's optical size support and geometric clarity make it ideal for a dense, data-rich dashboard. Weight 300 gives body text a light, readable feel; weight 800 is reserved exclusively for the hero title.
- **Monospace**: `'JetBrains Mono', monospace` — weights 300 through 500. JetBrains Mono is the technical voice of the system. It appears for stage numbers, data keys, section headers, and code blocks — never for prose. Its generous x-height reads clearly at 9–11px where many monospace fonts become illegible.

### Hierarchy

| Role | Font | Size | Weight | Line Height | Letter Spacing | Notes |
|------|------|------|--------|-------------|----------------|-------|
| Display Hero | Inter | clamp(36px, 5vw, 60px) | 800 | 1.05 | -0.04em | Largest headings only — weight 800 used nowhere else |
| Section Heading | Inter | 28px | 700 | 1.2 | -0.03em | Stage titles, major section markers |
| Sub-heading | Inter | 16px | 600 | 1.5 | normal | Card titles, panel headings |
| Body | Inter | 15px | 300 | 1.7 | normal | Standard body prose — light weight reads as meditative |
| Body Small | Inter | 14px | 400 | 1.8 | normal | Descriptions inside panels and tooltips |
| Body Tiny | Inter | 13px | 500 | 1.5 | normal | UI control labels, inline affordances |
| Caption | Inter | 12px | 500 | 1.4 | normal | Timestamps, metadata, footnotes |
| Eyebrow / Overline | JetBrains Mono | 11px | 500 | 1.3 | 0.12em | Uppercase stage numbers and section eyebrows |
| Mono Label | JetBrains Mono | 10px | 500 | 1.3 | 0.06–0.1em | Uppercase section headers, data keys in panels |
| Mono Tiny | JetBrains Mono | 9px | 400 | 1.3 | 0.15em | Smallest monospace — footer sub-labels only |
| Code Body | JetBrains Mono | 11px | 400 | 1.9 | normal | Code blocks, data panel content — generous line-height aids scanning |

### Principles
- **Weight as hierarchy**: The progression 300 → 400 → 500 → 600 → 700 → 800 maps directly to semantic importance. Weight 800 appears once per page, on the hero. Never use 800 for anything below Display Hero level.
- **Mono earns its place**: JetBrains Mono is not decorative — it signals "this is a key, a code, or a label." Using it for prose would dilute that signal. The two fonts maintain strict domain separation.
- **Generous body line-heights**: Body text at 1.7 and 1.8 gives the interface a contemplative quality appropriate to the gift-economy theme. Users reading about gift cycles should feel unhurried.
- **Uppercase is exclusive**: The `text-transform: uppercase` + `letter-spacing` treatment belongs only to JetBrains Mono at 9–11px. Never apply uppercase to Inter, and never apply it to body-size text.
- **Negative letter-spacing on large Inter**: Display and section headings use negative tracking (-0.03em to -0.04em) to counteract Inter's natural spacing at large sizes. This is essential — untracked large Inter reads as too loose and airy for this aesthetic.

## 4. Component Stylings

### Buttons

**Primary (Indigo Fill)**
- Background: `#818cf8` (Indigo)
- Text: `#09090b` (Deep Void) — dark text on the light-ish accent
- Padding: 10px 20px
- Border: none
- Radius: 8px (Default)
- Hover: `filter: brightness(1.1)`

**Outline (Indigo Border)**
- Background: transparent
- Text: `#818cf8` (Indigo)
- Border: `1px solid #818cf8`
- Padding: 10px 20px
- Radius: 8px
- Hover: `background: rgba(129,140,248,0.08)`

**Success (Emerald Fill)**
- Background: `#34d399` (Emerald)
- Text: `#09090b`
- Padding: 10px 20px
- Radius: 8px

**Warning (Amber Fill)**
- Background: `#fbbf24` (Amber)
- Text: `#09090b`
- Padding: 10px 20px
- Radius: 8px

**Glow (Gradient Background)**
- Background: `linear-gradient(135deg, #818cf8, #c084fc, #f472b6)`
- Text: `#ffffff`
- Box-shadow: `0 0 20px rgba(129,140,248,0.2)`
- Padding: 10px 24px
- Radius: 8px
- Reserved for the single highest-priority action on screen — submit a gift, complete a cycle

### Cards & Containers

**Desc Panel** (narrative containers, stage descriptions)
- Background: `#111114` (Dark Surface)
- Border: `1px solid #27272a` (Border Primary)
- Radius: 12px (Card)
- Padding: 24px
- Optional: left border `3px solid <stage-color>` with stage-tinted background at 4% opacity

**UI Mock** (product preview cards)
- Background: `#151518` (Card Surface)
- Top edge: `linear-gradient(90deg, transparent, rgba(129,140,248,0.2), transparent)` as 1px decorative top border
- Title bar strip: `#111114`, 10px JetBrains Mono label, Muted Zinc text
- Radius: 12px
- Border: `1px solid #27272a`

**Data Panel** (key-value displays, structured data)
- Background: `#111114` (Dark Surface)
- Header bar: `background: #151518`, `border-bottom: 1px solid #27272a`, 10px uppercase mono label
- Border: `1px solid #27272a`
- Radius: 12px
- Row items: `border-bottom: 1px solid #1c1c20`

**Token Card** (gift token, value display)
- Background: `rgba(244,114,182,0.04)` tint + `linear-gradient(135deg, rgba(129,140,248,0.08), rgba(244,114,182,0.08))`
- Top line: `linear-gradient(90deg, #818cf8, #c084fc, #f472b6)` as 2px top border
- Radius: 14px (Special)
- Token value: gradient text using the signature gradient

**Proof Card** (evidence, activity log entries)
- Background: stage-color tinted at 3–5% opacity
- Thumbnail: 48×48px, 8px radius, matching tint
- Border: `1px solid #27272a`
- Radius: 12px

### Forms

**Mock Input**
- Background: `#09090b` (Deep Void)
- Border: `1px solid #27272a`
- Radius: 8px (Default)
- Padding: 10px 14px
- Text: `#fafafa`; placeholder: `#52525b`
- Focus: `border-color: #818cf8`, `box-shadow: 0 0 0 3px rgba(129,140,248,0.15)`

**Mock Textarea**
- Same as Mock Input, `resize: vertical`, minimum height 80px

**Mock Label**
- Font: JetBrains Mono, 10px, weight 500
- Text: `#52525b` (Muted Zinc)
- `text-transform: uppercase`, `letter-spacing: 0.08em`
- Margin-bottom: 6px

### Stage Elements

**Stage Header** (section opener for each gift-cycle stage)
- Layout: flex row, `gap: 16px`, `align-items: center`
- Contains: stage icon + stage title + right-pointing arrow
- Bottom border: `1px solid #27272a`
- Stage number eyebrow: JetBrains Mono 11px, `letter-spacing: 0.12em`, uppercase, colored with stage accent

**Stage Icon**
- Size: 48×48px, radius 14px (between Card and Orb)
- Border: `2px solid <stage-color>`
- Background: `<stage-color>` at 8% opacity
- Emoji or icon at 24px, colored with stage accent
- Active state: `box-shadow: 0 0 30px <stage-color>@0.2`

**Narrative Block** (left-border accent panels)
- Left border: `3px solid <stage-color>`
- Background: `<stage-color>` at 4% opacity
- Radius: 0 12px 12px 0
- Padding: 16px 20px
- Body text in Soft Gray at 14px, 1.8 line-height

### Actor Badges
- Inline `<span>` with `background: <member-color>` at 12% opacity, `color: <member-color>`, `border-radius: 100px` (Pill)
- Font: Inter 12px, weight 600
- Padding: 2px 8px

### Tagged Badges (gradient accent)
- Border: `1px solid rgba(129,140,248,0.3)` — subtle gradient-adjacent border
- Background: `rgba(129,140,248,0.06)`
- Text: JetBrains Mono 10px, uppercase, `letter-spacing: 0.1em`, color `#818cf8`
- Optional shimmer animation: `@keyframes shimmer` sweeping a white highlight across the badge at low opacity
- Radius: 100px (Pill)

## 5. Layout Principles

### Spacing System
The base unit is **8px**. All spacing values are multiples or half-multiples of this unit. The full scale:
`2, 4, 6, 8, 10, 12, 14, 16, 20, 24, 32, 40, 60, 80px`

- **Container max-width**: 900px, horizontally centered
- **Side padding**: 32px on desktop, 20px on mobile
- **Section gap**: 80px between major page sections — the generous spacing gives each gift-cycle stage room to breathe
- **Component gap**: 24px between cards and panels within a section
- **Card internal padding**: 24px standard, 20px for compact variants

### Grid & Container
- Single-column layout for prose-heavy stage descriptions
- 2-column grid for stage detail panels on desktop (description + data), collapsing to single-column on mobile
- The cycle ring visualization is a 700px square, centered, using absolute positioning for nodes on a hexagonal layout
- Hero is centered single-column with the cycle ring below, then sequential stage sections

### Whitespace Philosophy
- **Section breathing room**: 80px vertical gaps create clear chapter separation as users scroll through the gift cycle. Each stage feels like entering a new room.
- **Dense within panels**: Card internals are compact but not cramped — 24px padding, 1.7 line-height, 12–16px component gaps inside panels.
- **Color over space**: Separation between zones comes primarily from background color shifts (Dark Surface vs. Card Surface vs. Deep Void) and 1px zinc borders — not from large whitespace gaps within components.

### Border Radius Scale
- **Sharp** (4px): Inline code spans, small tags, `<kbd>` elements — precision signals technical content
- **Default** (8px): Inputs, small buttons, compact containers — the everyday radius
- **Card** (12px): Standard cards and panels — the primary structural radius (`var(--radius)`)
- **Special** (14px): Token cards and blessing visuals — slightly softer for celebratory elements
- **Orb** (16px): Node orbs and stage icons — rounded enough to feel iconographic
- **Round** (50%): Avatars, member dots, pulse indicators — perfect circles only
- **Pill** (100px): State badges, actor tags, small status chips — infinite rounding for inline elements

## 6. Depth & Elevation

| Level | Treatment | Use |
|-------|-----------|-----|
| Flat (0) | No shadow, no border | Page background, inline body text |
| Contained (1) | `1px solid #27272a` | Standard panels — desc-panel, data-panel, narrative blocks |
| Card (2) | Background `#151518` + `1px solid #27272a` | Cards, UI mocks, elevated containers |
| Glow (3) | Colored `box-shadow: 0 0 30px <accent>@0.15` | Active stage nodes, focused inputs, selected cards |
| Gradient (4) | `box-shadow: 0 0 20px rgba(129,140,248,0.2)` | Glow buttons, hero gradient accent elements |

**Shadow Philosophy**: Depth in this system comes from background color stepping and border presence — not from drop shadows. The three background values (Deep Void, Dark Surface, Card Surface) create a natural z-axis when layered. Colored glows at Levels 3 and 4 are the exception: they signal active state and interactivity, referencing the "jewels emitting light" imagery of Indra's Network.

### Decorative Effects

**Dot Grid** — applied once, to `body::before` as a `position: fixed` overlay. `radial-gradient(circle, rgba(129,140,248,0.05) 1px, transparent 1px)` at `24px 24px` background-size. The grid is a page-level texture — never applied to nested containers, where it would create visual noise and undermine the single source of depth texture.

**Gradient Top Edge** — `linear-gradient(90deg, transparent, rgba(129,140,248,0.2), transparent)` as a 1px pseudo-element on UI mock components. Creates the impression of an illuminated panel edge without an explicit border color.

**Gradient Text** — `background: linear-gradient(135deg, #818cf8, #c084fc, #f472b6); -webkit-background-clip: text; -webkit-text-fill-color: transparent`. Applied to hero title emphasis spans and token value displays. Never used on body text or text smaller than 16px.

**Stage Tints** — panel backgrounds tinted with the active stage color at 3–6% opacity. Example: an Attention-themed panel uses `background: rgba(251,191,36,0.04)`. This subtly shifts the panel's color temperature to match its stage without overwhelming the neutral palette.

**Pulse Animation** — `animation: pulse 2s ease-in-out infinite` with `opacity` oscillating between 0.4 and 1.0. Used for attention-demanding indicators: active stage markers in the cycle ring, live presence dots, pending blessing states.

**Hero Glow** — a large radial gradient behind the hero section: `radial-gradient(circle, rgba(129,140,248,0.06) 0%, rgba(192,132,252,0.03) 40%, transparent 70%)`. Subtle ambient light that frames the hero without competing with content.

## 7. Do's and Don'ts

### Do
- Use Deep Void (`#09090b`) as the primary background — the entire depth system depends on this being the darkest layer
- Apply the dot-grid texture at the page level via `body::before`; it is a single atmospheric layer, not a card-level decoration
- Use gradient text sparingly — only for hero emphasis spans and token value displays where the visual weight is justified
- Keep JetBrains Mono labels uppercase with positive letter-spacing (0.06–0.15em) — this is how the two fonts stay in separate semantic lanes
- Use stage colors consistently and only for their designated stage — Amber is Attention, always; Emerald is Service, always
- Tint panel backgrounds with stage colors at 3–6% opacity to create chromatic atmosphere within each stage section
- Keep body line-heights at 1.7 or higher — the reading experience should feel unhurried
- Use Border Primary (`#27272a`) for all container borders; use Border Subtle (`#1c1c20`) for internal row separators
- Reserve the signature gradient (`#818cf8 → #c084fc → #f472b6`) for single highest-emphasis moments on screen

### Don't
- Don't use bright or white backgrounds for main surfaces — this is a nocturnal interface, and any white panel will shatter the immersion
- Don't apply gradient text to body copy or text below 16px — the effect becomes illegible and decorative noise at small sizes
- Don't mix stage colors arbitrarily — assigning Amber to a Blessing interaction, or Pink to an Attention state, breaks the semantic system users rely on
- Don't use heavy drop shadows for depth — depth comes from background layering (`#09090b` → `#111114` → `#151518`) and 1px zinc borders
- Don't use weight 800 outside of the Display Hero — Inter at 800 is a structural anchor, not a general emphasis tool
- Don't use warm grays — the neutral scale is zinc-based (cool undertone). Introducing stone-family or beige-cast colors will clash with the zinc cast
- Don't exceed border-radius 16px on content cards — exceptions are pills (100px) and perfect circles (50%) only
- Don't apply the dot-grid to nested containers, panels, or cards — it is a page-level texture and must remain the background layer
- Don't use JetBrains Mono for prose — mono signals data, labels, and code. Using it for narrative text dilutes the dual-font semantic contract

## 8. Responsive Behavior

### Breakpoints

| Name | Width | Key Changes |
|------|-------|-------------|
| Mobile | <768px | Single column, 20px side padding, stage grids stack vertically, cycle ring hidden, hero scales to ~32px |
| Desktop | ≥768px | 900px container, 32px padding, 2-column stage grids, full cycle ring visualization, hero at full clamp size |

The system uses a single breakpoint at 768px. Below this threshold, the layout simplifies dramatically to prioritize content readability on small screens.

### Mobile Adaptations
- **Hero title**: Scales from `clamp(36px, 5vw, 60px)` — at 360px viewport this renders around 32px, which is appropriate
- **Cycle ring**: Hidden on mobile (the hexagonal ring requires sufficient space to be legible). Stage detail sections stand alone as the primary navigation on small screens
- **Stage grids**: The 2-column description + data layout collapses to single-column; desc-panel stacks above data-panel
- **Side padding**: Reduces from 32px to 20px — maintaining breathing room on the smallest devices
- **Component spacing**: Section gap reduces from 80px to 48px; component gap from 24px to 16px

### Touch Targets
- All interactive elements (buttons, stage nodes, cycle-ring navigation) meet a 44×44px minimum touch target
- Stage nodes in the cycle ring are 120×120px — well above minimum
- Node orbs are 64×64px — adequate for touch

### Collapsing Strategy
- **Cycle ring** → hidden below 768px; stage sections serve as the linear equivalent
- **2-column panels** → stack to single column
- **Data panels** → rows maintain full width; font sizes remain constant for data legibility
- **Hero sub-heading** → `max-width: 100%` on mobile (removes the 540px desktop cap)

## 9. Agent Prompt Guide

### Quick Color Reference
- Page background: Deep Void (`#09090b`)
- Panel background: Dark Surface (`#111114`)
- Card background: Card Surface (`#151518`)
- Hover state: Hover Surface (`#1c1c20`)
- Primary text: Pure Light (`#fafafa`)
- Body text: Soft Gray (`#a0a0ab`)
- Metadata text: Muted Zinc (`#52525b`)
- Container border: Border Primary (`#27272a`)
- Row separator: Border Subtle (`#1c1c20`)
- Primary accent: Indigo (`#818cf8`)
- Secondary accent: Purple (`#c084fc`)
- Tertiary accent: Pink (`#f472b6`)
- Signature gradient: `linear-gradient(135deg, #818cf8, #c084fc, #f472b6)`
- Success / Service: Emerald (`#34d399`)
- Warning / Attention: Amber (`#fbbf24`)
- Error: Red (`#f87171`)

### Example Component Prompts

- "Create a stage detail card for the Attention stage. Use Dark Surface (`#111114`) as background with a `1px solid #27272a` border and 12px radius. Add a left border in Amber (`#fbbf24`) at 3px width. Tint the background with `rgba(251,191,36,0.04)`. Use an Amber-colored stage icon (48px, 14px radius, 2px Amber border, 8% opacity Amber fill). Title in Pure Light at 28px Inter weight 700, stage number eyebrow in JetBrains Mono 11px uppercase with 0.12em letter-spacing in Amber."

- "Design a gift token card displaying a token value. Background: `rgba(244,114,182,0.04)` with a 2px top border using the signature gradient (`linear-gradient(90deg, #818cf8, #c084fc, #f472b6)`). Token value displayed in 28px Inter weight 700 using gradient text (`background: linear-gradient(135deg, #818cf8, #c084fc, #f472b6); -webkit-background-clip: text; -webkit-text-fill-color: transparent`). Below it, a JetBrains Mono 10px uppercase label in Muted Zinc. Card radius 14px, border `1px solid #27272a`."

- "Build a hero section on Deep Void (`#09090b`) with the dot-grid body background (`radial-gradient(circle, rgba(129,140,248,0.05) 1px, transparent 1px)` at 24px). Eyebrow in JetBrains Mono 11px uppercase Indigo with 0.12em letter-spacing. Headline at `clamp(36px, 5vw, 60px)`, Inter weight 800, line-height 1.05, letter-spacing -0.04em — with the key phrase wrapped in gradient text (indigo → purple → pink). Sub-heading in Soft Gray at 16px Inter weight 300, max-width 540px. Primary glow button below using the signature gradient background with `box-shadow: 0 0 20px rgba(129,140,248,0.2)`."

- "Create a data panel for displaying gift-cycle metrics. Background Dark Surface (`#111114`), border `1px solid #27272a`, 12px radius. Header bar in Card Surface (`#151518`) with `border-bottom: 1px solid #27272a`. Header label in JetBrains Mono 10px uppercase Muted Zinc with 0.08em letter-spacing. Data rows use `border-bottom: 1px solid #1c1c20`. Key labels in JetBrains Mono 10px weight 500 Muted Zinc; values in Inter 14px weight 400 Pure Light."

- "Design an inline actor badge for a member named 'Joy'. Use `rgba(255,217,61,0.12)` background and `#ffd93d` text, `border-radius: 100px`, padding 2px 8px, Inter 12px weight 600. Embed it inline within a sentence of Soft Gray body text at 15px Inter weight 300."

- "Build a cycle ring node for the Service stage. Outer container 120×120px centered. Inner orb 64×64px, `border-radius: 16px`, `border: 2px solid #34d399`, background `rgba(52,211,153,0.1)`, emoji icon at 28px. Active state: `box-shadow: 0 0 30px rgba(52,211,153,0.2)` and a pulse ring (`::before` at `inset: -6px`, `border-radius: 20px`, `border: 1px solid #34d399`, `opacity: 0.3`). Node label below in JetBrains Mono 10px uppercase `#34d399` with 0.06em letter-spacing."

### Iteration Guide
When refining existing screens generated with this design system:
1. Reference specific color names and hex values from this document — "use Soft Gray (`#a0a0ab`)" not "make it lighter"
2. Stage colors carry semantic meaning — always identify which stage a component belongs to before choosing accent colors
3. Describe depth in terms of background layers: "put this on Card Surface (`#151518`) inside a Dark Surface (`#111114`) panel" rather than "add a shadow"
4. For glow effects: "add an Indigo glow — `box-shadow: 0 0 30px rgba(129,140,248,0.15)`"
5. For gradient text: "apply the signature gradient as background-clip text on this heading span"
6. Font attribution: Inter for all prose and headings; JetBrains Mono only for labels, keys, stage numbers, and code — never swap them
7. Stage tinting: "tint this panel with the Blessing stage color at 4% opacity — `rgba(192,132,252,0.04)`"
