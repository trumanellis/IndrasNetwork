# Design System: SyncEngine

## 1. Visual Theme & Atmosphere

SyncEngine's website is a dark, organic, bioluminescent experience — a near-black forest floor (`#080a08`) where content glows softly into existence like moss catching moonlight. The overall impression is one of living technology: a system that breathes, pulses, and grows rather than one built from cold precision. This is not a tech startup dark theme — it is darkness as ecosystem, where information emerges through gradients of natural light rather than hard contrast.

The typography system pairs two deeply intentional families. `Outfit` serves as the display/UI face — a geometric sans-serif used at extreme weights (700–900) for headlines that feel carved and monumental, with aggressive negative letter-spacing (-0.03em to -0.04em) that compresses text into dense, powerful blocks. `Source Serif 4` is the body face — an optical-sizing serif with variable weight that brings warmth, readability, and literary gravitas to prose. This pairing of geometric display + humanist serif creates a distinctive tension: the headlines feel engineered, the body text feels authored. `IBM Plex Mono` completes the triad as the monospace companion for labels, metadata, and technical asides — always at small sizes (9–12px), always uppercase with wide letter-spacing, functioning as quiet structural annotations.

The color system draws from nature's palette rather than brand primaries. A bioluminescent moss green (`#8aad6e`) anchors the gift economy and primary interactions. An indigo-violet (`#818cf8`) represents identity and quantum security. These two colors meet in gradient combinations that feel like aurora — not artificial brand expression but light refracting through atmosphere. Supporting accents (rose `#f07eb8`, amber `#f0c87e`, teal `#7edcf0`, pulse green `#34d399`) are used sparingly for stage-specific theming, each tied to a conceptual domain within the SyncEngine protocol.

The atmosphere layer is the site's most distinctive visual element: fixed-position, heavily blurred orbs (`filter: blur(100px)`) drift slowly across the viewport in a `20s ease-in-out infinite alternate` animation, creating a sense of living, breathing depth behind all content. A fractal noise grain texture overlays everything at 3% opacity, adding organic texture to the digital canvas. Together they produce the feeling of looking through bioluminescent water.

**Key Characteristics:**
- Dark organic canvas: `#080a08` base, `#0f120f` secondary, `#131813` card, `#1a1f1a` surface — all with green undertones
- `Outfit` for display at weight 800–900 with -0.03em to -0.04em letter-spacing — bold, compressed, monumental
- `Source Serif 4` for body at weight 300 with optical sizing — warm, literary, human
- `IBM Plex Mono` for labels at 9–12px, uppercase, wide-tracked — quiet structural annotations
- Bioluminescent moss (`#8aad6e`) as primary accent, indigo (`#818cf8`) as secondary — nature over brand
- Floating atmosphere orbs with 100px blur, 20s drift animation — living background
- Fractal noise grain at 3% opacity — organic digital texture
- 16px default border-radius — generous, organic rounding
- Pill-shaped elements (100px radius) for hero buttons and badges — soft, biological forms
- Reveal animations: 36px translateY with 0.9s cubic-bezier(0.16, 1, 0.3, 1) — elements rise into view like growing things

## 2. Color Palette & Roles

### Background Surfaces
- **Base** (`#080a08`): `--bg`. The deepest background — a near-black with a green undertone that reads as dark earth or deep forest.
- **Secondary** (`#0f120f`): `--bg2`. Panel interiors, aside backgrounds, expandable sections. One step lighter with the same green cast.
- **Tertiary** (`#141918`): `--bg3`. Hover states for threat cards, slightly elevated surfaces.
- **Card** (`#131813`): `--card`. Card and panel backgrounds — distinct from bg2, calibrated for the specific elevation of content containers.
- **Surface** (`#1a1f1a`): `--surface`. The lightest dark surface — elevated interactive elements.

### Text Hierarchy
- **Primary** (`#eaede8`): `--tx`. Near-white with a warm, slightly green cast. Default text, headings, strong emphasis. Not pure white — prevents harshness against the organic backgrounds.
- **Secondary** (`#b0b8a8`): `--t2`. Sage-tinted gray for body text, descriptions, and secondary content. The green undertone ties it to the ecosystem palette.
- **Tertiary** (`#8a9480`): `--t3`. Muted moss-gray for labels, metadata, nav links, and de-emphasized text.
- **Dim** (`#5e6858`): `--dim`. The most subdued text — timestamps, footer copy, monospace annotations, disabled states.

### Bioluminescent Moss (Gift Cycle / Primary)
- **Moss** (`#8aad6e`): `--moss`. Primary accent — CTA interactions, focus rings, gift cycle theming, active nav dots. The heart of the color system.
- **Moss Light** (`#a4c48a`): `--moss-light`. Lighter variant for link text, hover states, gradient endpoints. Used in the logo gradient.
- **Moss Dim** (`#5c7a48`): `--moss-dim`. Darker moss for pressed states and muted accents.
- **Glow Green** (`#7ef0b8`): `--glow-green`. Bright bioluminescent green for highlights and glow effects.
- **Glow Amber** (`#f0c87e`): `--glow-amber`. Warm amber for timer elements, warning states, and the "Your Time" stage.
- **Glow Teal** (`#7edcf0`): `--glow-teal`. Cool teal for "The Network" stage and secondary highlights.

### Indigo Quantum (Identity / Security)
- **Indigo** (`#818cf8`): `--indigo`. Secondary accent — identity/security theming, stage 1 color, gradient endpoints. The cooler counterpart to moss.
- **Indigo Light** (`#a5b4fc`): `--indigo-light`. Lighter indigo for gradient highlights and the logo gradient's cool end.
- **Indigo Dim** (`#6366f1`): `--indigo-dim`. Deeper indigo for hover states on indigo elements.

### Violet & Rose
- **Violet** (`#b48efa`): `--violet`. Rich purple for stage 5, gradient midpoints, encounter codes, and oracle theming.
- **Violet Light** (`#c4b5fd`): `--violet-light`. Softer violet for badges and subtle accents.
- **Rose** (`#f07eb8`): `--rose`. Pink accent for stage 6, gradient decorations, and the rose button variant.
- **Rose Light** (`#f9a8d4`): `--rose-light`. Lighter rose for subtle tinting.

### Functional
- **Pulse** (`#34d399`): `--pulse`. Emerald green for "alive" indicators — heartbeat visuals, status dots, check marks.
- **Pulse Light** (`#6ee7b7`): `--pulse-light`. Lighter pulse for softer status indicators.
- **Danger** (`#ef4444`): `--danger`. Red for threat cards, error states, bot detection warnings.

### Stage Colors (Sequential Theming)
- **S1** (`#818cf8`): Indigo — Identity / "Your Need"
- **S2** (`#7eb8f0`): Sky blue — Discovery
- **S3** (`#f0c87e`): Amber — "Your Time"
- **S4** (`#8aad6e`): Moss — "Your Gift"
- **S5** (`#b48efa`): Violet — "Your People"
- **S6** (`#f07eb8`): Rose — "Your Story"
- **S7** (`#7edcf0`): Teal — "The Network"

### Border System
- **Border Primary** (`#242e22`): `--bd`. Default border — dark green-tinted separator for cards, panels, and sections.
- **Border Secondary** (`#2e3a2c`): `--bd2`. Slightly lighter border for interactive elements, profile cards, phone mockups.

## 3. Typography Rules

### Font Families
- **Display**: `'Outfit', sans-serif` — geometric sans-serif for headings, buttons, labels, and navigation
- **Body**: `'Source Serif 4', Georgia, serif` — variable optical-sizing serif for prose and reading text
- **Monospace**: `'IBM Plex Mono', monospace` — for technical labels, metadata, and code

### Hierarchy

| Role | Font | Size | Weight | Line Height | Letter Spacing | Notes |
|------|------|------|--------|-------------|----------------|-------|
| Hero Display | Outfit | clamp(36px, 7vw, 78px) | 900 | 1.0 (tight) | -0.04em | Maximum impact, gradient text via `-webkit-background-clip` |
| Stage Heading | Outfit | clamp(30px, 5vw, 56px) | 800 | 1.1 (tight) | -0.03em | Section titles, bold and compressed |
| Section Heading (h2) | Outfit | clamp(1.3rem, 3vw, 1.8rem) | 600 | 1.4 | normal | Whitepaper/article h2s |
| Feature Heading | Outfit | clamp(24px, 3vw, 36px) | 800 | 1.15 | -0.03em | Feature section titles |
| Card Heading | Outfit | 20–24px | 700 | 1.2 | -0.02em | Card titles, profile names, token titles |
| Sub-heading | Outfit | 16–17px | 700 | 1.2 | -0.02em | Small card headings, metadata headings |
| Body | Source Serif 4 | 17px (1.06rem) | 300–400 | 1.75–1.9 | normal | Primary reading text, optical sizing enabled |
| Body Large | Source Serif 4 | 18px | 300 | 1.9 | normal | Hero subtitles, feature descriptions |
| Body Small | Source Serif 4 | 15–16px | 300 | 1.85 | normal | Feature paragraphs, card descriptions |
| Button | Outfit | 14–16px | 600 | 1.0 | normal | CTA buttons, hero buttons |
| Nav Link | Outfit | 13px | 500 | 1.0 | normal | Site navigation links |
| Stage Label | IBM Plex Mono | 10–11px | 400–500 | 1.0 | 0.10–0.14em | Uppercase stage numbers, feature labels |
| Metadata | IBM Plex Mono | 11–12px | 300–500 | 1.7–1.8 | 0.04–0.08em | Token IDs, timestamps, badge text, expandable headers |
| Micro Label | IBM Plex Mono | 9–10px | 400 | 1.0 | 0.08–0.12em | Uppercase tiny labels, chart annotations, footer copy |
| Code Inline | IBM Plex Mono | 0.88em | 500 | inherit | normal | Inline code within prose |
| Code Block | IBM Plex Mono | 0.85rem | 400 | 1.6 | normal | Code blocks in articles |
| Tech Aside | IBM Plex Mono | 11–12px | 400 | 1.7–1.8 | normal | Technical sidebar annotations |

### Principles
- **Three-font system with clear roles**: Outfit = structure (headings, UI), Source Serif 4 = content (prose, body), IBM Plex Mono = annotation (labels, metadata, code). Never mix roles.
- **Extreme weight contrast**: Outfit runs 500–900 (heavy), Source Serif 4 runs 300–400 (light), IBM Plex Mono runs 300–500 (medium). This weight disparity creates natural hierarchy without size escalation.
- **Negative tracking for display, positive for labels**: Display headings compress with -0.03em to -0.04em. Mono labels expand with 0.06em to 0.14em. This creates visual tension between headline density and label airiness.
- **Optical sizing**: Source Serif 4's `font-optical-sizing: auto` adjusts letterforms based on rendered size — thinner strokes at body sizes, more contrast at display.
- **Uppercase is monospace-only**: Only IBM Plex Mono text uses `text-transform: uppercase`. Outfit and Source Serif 4 are always mixed-case.
- **Generous body line-height**: 1.75–1.9 for body text — significantly more generous than typical (1.4–1.6), creating a meditative, breathable reading experience.

## 4. Component Stylings

### Buttons

**Hero Primary (Pill)**
- Background: `linear-gradient(135deg, var(--moss), var(--indigo))`
- Text: `white`, 16px Outfit weight 600
- Padding: 16px 40px
- Radius: 100px (full pill)
- Shadow: `0 6px 36px rgba(138,173,110,0.2)`
- Hover: translateY(-3px), shadow intensifies to `0 10px 48px rgba(138,173,110,0.3)`
- Use: Primary hero CTA

**Hero Ghost (Pill)**
- Background: transparent
- Text: `var(--t2)`, 15px Outfit weight 500
- Padding: 16px 32px
- Radius: 100px (full pill)
- Border: `1px solid var(--bd2)`
- Hover: border-color `var(--t3)`, text color `var(--tx)`
- Use: Secondary hero CTA

**Standard Button**
- Padding: 13px 30px
- Radius: 12px
- Font: 14px Outfit weight 600
- Hover: translateY(-2px)
- Variants:
  - `.btn-moss`: solid `var(--moss)`, dark text
  - `.btn-amber`: solid `var(--glow-amber)`, dark text
  - `.btn-indigo`: `linear-gradient(135deg, var(--indigo), var(--violet))`, white text, `0 4px 24px rgba(129,140,248,0.2)` shadow
  - `.btn-rose`: `linear-gradient(135deg, var(--rose), var(--violet))`, white text, `0 4px 24px rgba(240,126,184,0.2)` shadow
  - `.btn-final`: `linear-gradient(135deg, var(--moss), var(--indigo), var(--violet))`, white text — the three-color ecosystem gradient

### Cards & Panels

**Standard Panel**
- Background: `var(--card)` (`#131813`)
- Border: `1px solid var(--bd)` (`#242e22`)
- Radius: `var(--radius)` (16px)
- Padding: 32px
- Overflow: hidden
- Optional accent: 2px-tall gradient strip at top via `.panel-accent`

**Threat Card**
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`
- Radius: `var(--radius)` (16px)
- Hover: border-color `rgba(239,68,68,0.4)`, background `var(--bg3)`
- Icon container: 40x40px, 10px radius, danger-tinted background

**Profile Card**
- Background: `var(--card)`
- Border: `1px solid var(--bd)`
- Radius: 20px (larger than standard)
- Padding: 36px
- Top glow: 3px gradient strip (`--moss`, `--indigo`, `--violet`, `--rose`)
- Avatar: 80px circle with gradient background and outer ring border

**Token of Gratitude**
- Background: `var(--card)`
- Border: `1px solid var(--bd2)`
- Radius: 20px
- Top glow: 3px gradient (`--moss`, `--glow-amber`, `--violet`, `--rose`)
- Inner radial gradient overlay at 4% opacity for depth
- State pill: pulsing `box-shadow` animation (3s cycle)

**CTA Card (Linkable)**
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`, 2px solid colored top border
- Radius: `var(--radius)` (16px)
- Hover: translateY(-3px), `0 6px 20px rgba(0,0,0,0.25)` shadow

### Badges & Chips

**Monospace Badge/Pill**
- Background: color-tinted at ~8% opacity (e.g., `rgba(180,142,250,0.08)`)
- Border: `1px solid` color-tinted at ~20% opacity
- Padding: 6px 18px
- Radius: 100px (full pill)
- Font: IBM Plex Mono, 10–11px, uppercase, 0.08em letter-spacing

**Interactive Chip**
- Padding: 8px 18px
- Radius: 100px (full pill)
- Border: `1px solid var(--bd)`
- Font: 13px Outfit weight 500, color `var(--t3)`
- Hover: border-color `var(--bd2)`, color `var(--t2)`
- Active (`.on`): border-color `var(--moss)`, color `var(--moss-light)`, background `rgba(138,173,110,0.08)`

### Inputs & Forms
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`
- Radius: 12px
- Padding: 14px 18px
- Font: 16px Source Serif 4
- Focus: border-color `var(--moss)`, box-shadow `0 0 0 3px rgba(138,173,110,0.1)`
- Label: 12px Outfit weight 600, uppercase, 0.06em tracking, color `var(--t3)`
- Placeholder: color `var(--dim)`
- Textarea: min-height 110px, resize vertical

### Expandable Panels (Details/Summary)
- Container: `1px solid var(--bd)`, 16px radius
- Summary: 14px 20px padding, IBM Plex Mono 11px, `var(--t3)` color
- Arrow: `▸` character, rotates 90deg on open via CSS transform
- Body: `var(--bg2)` background, 1px border-top, IBM Plex Mono 12px

### Navigation
- Fixed top, 56px height
- Background: `rgba(8,10,8,0.85)` with `backdrop-filter: blur(12px)`
- Border-bottom: `1px solid var(--bd)`
- Logo: Outfit 18px weight 800, "Sync" in `var(--tx)`, "Engine" in moss-to-indigo gradient
- Links: Outfit 13px weight 500, `var(--t3)`, hover/active `var(--tx)`
- Link gap: 28px
- Mobile: hamburger toggle, links drop down as vertical column with blurred background

### Site Footer
- Border-top: `1px solid var(--bd)`
- Padding: 48px 24px, centered
- Links: Outfit 13px, `var(--t3)`, horizontal flex with 24px gap
- Copy: IBM Plex Mono 11px, `var(--dim)`, 0.04em tracking

### Aside / Note
- Background: `var(--bg2)`
- Border: `1px solid var(--bd)`
- Radius: 16px
- Padding: 24px
- Header: Outfit 11px weight 700, uppercase, 0.08em tracking, `var(--dim)`
- Body: 15px Source Serif 4, `var(--t2)`

**Tech Aside (Code-flavored)**
- Same as note but with `border-left: 3px solid` (color varies by context)
- Font: IBM Plex Mono 11px
- Inline code: `rgba(129,140,248,0.08)` background, 3px radius

## 5. Layout Principles

### Spacing System
- Base: 8px inferred from component padding patterns
- Common values: 4px, 6px, 8px, 10px, 12px, 14px, 16px, 18px, 20px, 24px, 28px, 32px, 36px, 40px, 48px, 56px, 64px, 80px, 100px
- The scale is dense at the small end (label/badge padding) and expands dramatically for section spacing

### Grid & Container
- Standard wrap: `max-width: 1060px`, 32px horizontal padding
- Narrow wrap: `max-width: 780px`, 24px horizontal padding
- Wide layout (network page): `max-width: 1200px`, 2rem horizontal padding
- Whitepaper: `max-width: 960px`, 3rem 2rem padding

### Section Structure
- Full-height sections (`.sect`): `min-height: 100vh`, flex column centered, 100px vertical padding
- Short sections (`.sect-short`): `min-height: auto`, 80px vertical padding
- Sections stack vertically — no side-by-side page layouts
- Feature pairs: stacked text + visual with 28–36px gap (mobile-first, no side-by-side default)

### Whitespace Philosophy
- **Breathing room**: Body text at 1.75–1.9 line-height creates unusually generous vertical rhythm — the site reads slowly and deliberately, like a meditation.
- **Section as experience**: Full-viewport sections create discrete "rooms" that the reader enters one at a time, enhanced by scroll-linked nav dots and a progress bar.
- **Dense technical, generous narrative**: Technical asides and data displays (structs, formulas, decay charts) are compact and mono-spaced, while narrative prose gets maximum breathing room. This creates a natural foreground/background layering.

### Border Radius Scale
- Tight (3px): Inline code badges, small technical elements
- Standard (10–12px): Buttons, inputs, icon containers, small cards
- Comfortable (16px): `var(--radius)` — the workhorse. Panels, notes, expandable sections, feature visuals, metabolism cards
- Large (20px): Profile cards, token cards, phone mockups — premium, featured elements
- Round (24px): Phone screen mockups
- Pill (100px): Hero buttons, badge pills, chips — fully rounded biological forms

## 6. Depth & Elevation

| Level | Treatment | Use |
|-------|-----------|-----|
| Flat (Level 0) | No shadow, `var(--bg)` background | Page canvas, section backgrounds |
| Subtle (Level 1) | Tinted background only (`var(--bg2)`) | Notes, asides, expandable bodies, form inputs |
| Card (Level 2) | `1px solid var(--bd)` border, `var(--card)` background | Standard panels, threat cards, feature visuals |
| Elevated (Level 3) | `0 8px 40px rgba(0,0,0,0.4)` | Phone mockups, prominent interactive cards |
| Hero CTA (Level 4) | `0 6px 36px rgba(138,173,110,0.2)` — moss-tinted | Hero buttons; hover intensifies to `0 10px 48px` |
| Gradient Glow | `0 4px 24px rgba(color,0.2)` | Gradient buttons (indigo, rose, final) |
| Atmosphere | `filter: blur(100px)` on background orbs | Floating atmosphere orbs — environmental depth |
| Focus Ring | `2px solid var(--moss)`, 3px offset | Keyboard focus indicator (`:focus-visible`) |

**Depth Philosophy**: SyncEngine avoids traditional box-shadow elevation. Instead, depth is created through three complementary systems: (1) **background color layering** — the green-tinted gray scale (`#080a08` → `#0f120f` → `#131813` → `#1a1f1a`) creates subtle elevation through luminance alone; (2) **border presence** — 1px solid borders in `var(--bd)` define containment without shadow; (3) **atmospheric blur** — the background orbs at 100px blur create a sense of z-depth that makes content feel like it's floating above a living substrate. Actual `box-shadow` is reserved for high-emphasis elements (hero CTAs, phone mockups) and always uses either black (`rgba(0,0,0,0.4)`) or the moss accent (`rgba(138,173,110,0.2)`), never neutral gray.

### Decorative Depth
- **Gradient top strips**: 2–3px gradient bars at the top of cards (profile, token) act as colored elevation indicators
- **Radial gradient overlays**: Subtle `radial-gradient` overlays at ~3–7% opacity on hero sections and feature visuals add atmospheric depth
- **Dot grid patterns**: `radial-gradient(circle, rgba(129,140,248,0.03) 1px, transparent 1px)` at 20px spacing for technical/feature visual backgrounds
- **Animated pulse rings**: `box-shadow` animation on state indicators — `0 0 0 0` to `0 0 0 6–8px` at 0% opacity, creating a biological "breathing" effect

## 7. Do's and Don'ts

### Do
- Use the three-font system consistently: Outfit for structure, Source Serif 4 for prose, IBM Plex Mono for annotations
- Use weight 800–900 for Outfit display headings — the heaviness IS the brand voice
- Use weight 300 for Source Serif 4 body text — lightness creates the meditative reading feel
- Apply green-undertone backgrounds (`#080a08`, `#0f120f`, `#131813`) — the organic tint matters
- Use `var(--tx)` (`#eaede8`) for primary text — warm near-white, never pure `#ffffff` for body text
- Keep border-radius at 16px for panels and 100px for pills — two clear tiers of rounding
- Use moss green (`#8aad6e`) as the primary interactive accent
- Use the 135deg gradient direction for all multi-color gradients — it's consistent throughout
- Apply `backdrop-filter: blur(12px)` on overlaid surfaces (nav, mobile menu)
- Use uppercase + wide letter-spacing only on IBM Plex Mono labels
- Use the reveal animation (`.r` class) for scroll-triggered content entrance
- Keep body line-height at 1.75–1.9 — the generous spacing is intentional

### Don't
- Don't use Source Serif 4 for headings or buttons — it's exclusively the body/prose font
- Don't use Outfit for long-form reading text — it's for structure and UI only
- Don't use pure black (`#000000`) or pure white (`#ffffff`) for backgrounds or text — always use the green-tinted variants
- Don't use neutral gray borders — borders are green-tinted (`#242e22`, `#2e3a2c`)
- Don't use box-shadow for standard card elevation — use background color steps and 1px borders instead
- Don't apply uppercase to Outfit or Source Serif 4 text — uppercase is reserved for monospace labels
- Don't use border-radius values between 16px and 100px (the 20–24px exception is only for featured/premium cards like profiles and tokens)
- Don't use stage colors (S1–S7) outside their designated sections — each color is tied to a specific narrative stage
- Don't use danger red (`#ef4444`) as a brand accent — it's reserved for threats and errors
- Don't add drop shadows to every card — most cards get depth from border + background color alone
- Don't break the atmosphere: avoid hard edges, high-contrast borders, or any element that disrupts the organic, bioluminescent feel

## 8. Responsive Behavior

### Breakpoints
| Name | Width | Key Changes |
|------|-------|-------------|
| Mobile | <768px | Single column, nav collapses to hamburger, reduced section padding, nav dots hidden |
| Small Mobile | <640px | Tighter horizontal padding (1.5rem), further reduced spacing |
| Desktop | >768px | Full layout, horizontal nav, nav dots visible, full section padding |

### Touch Targets
- Hero buttons: 16px vertical padding, generous 40px horizontal — large and comfortable
- Standard buttons: 13px 30px — comfortably tappable
- Nav links: 13px font with 28px gaps — adequate spacing
- Chips: 8px 18px — minimum comfortable touch area
- Hamburger toggle: adequate padding (4px 8px) with 20px font size

### Collapsing Strategy
- **Hero**: clamp(36px, 7vw, 78px) scales fluidly — no abrupt breakpoint jumps
- **Stage headings**: clamp(30px, 5vw, 56px) — same fluid scaling approach
- **Navigation**: horizontal links + gaps → hamburger toggle + vertical dropdown with blurred overlay
- **Nav dots**: hidden entirely on mobile (touch scrolling replaces dot navigation)
- **Threat grid**: 3-column → single column
- **Bot comparison**: 2-column → single column
- **Section padding**: 100px → 64px on mobile; short sections 80px → 48px
- **Profile cards**: 36px padding → 24px
- **Whitepaper**: 3rem 2rem padding → 2rem 1.25rem

### Animation Behavior
- `prefers-reduced-motion: reduce`: all reveal animations disabled (opacity: 1, transform: none), all animation and transition durations set to 0.01ms
- This is a hard disable — no partial reduction, fully respects user preference

## 9. Agent Prompt Guide

### Quick Color Reference
- Primary accent: Moss Green (`#8aad6e`)
- Secondary accent: Indigo (`#818cf8`)
- Background: Dark Earth (`#080a08`)
- Card background: Dark Forest (`#131813`)
- Panel background: Deep Moss (`#0f120f`)
- Heading text: Warm White (`#eaede8`)
- Body text: Sage Gray (`#b0b8a8`)
- Tertiary text: Muted Moss (`#8a9480`)
- Dim text: Dark Sage (`#5e6858`)
- Border: Forest Edge (`#242e22`)
- Link: Moss Light (`#a4c48a`)
- Violet accent: Purple (`#b48efa`)
- Rose accent: Pink (`#f07eb8`)
- Pulse/alive: Emerald (`#34d399`)
- Danger: Red (`#ef4444`)
- Amber: Warm Gold (`#f0c87e`)

### Example Component Prompts
- "Create a hero section on `#080a08` background with floating blurred orbs (600px, `rgba(138,173,110,0.07)`, `blur(100px)`). Headline at clamp(36px, 7vw, 78px) Outfit weight 900, line-height 1.0, letter-spacing -0.04em, color `#eaede8`. Key words use `background: linear-gradient(135deg, #a4c48a, #a5b4fc, #b48efa)` with `-webkit-background-clip: text`. Subtitle at 18px Source Serif 4 weight 300, line-height 1.9, color `#b0b8a8`. Primary CTA pill button (moss-to-indigo gradient, 100px radius, 16px 40px padding, white text, Outfit 16px/600). Ghost pill (transparent, 1px solid `#2e3a2c`, `#b0b8a8` text, 100px radius)."
- "Design a card panel: `#131813` background, 1px solid `#242e22` border, 16px radius, 32px padding. Optional 2px gradient accent strip at top. Title at 20px Outfit weight 700, letter-spacing -0.02em, color `#eaede8`. Body at 15px Source Serif 4 weight 300, color `#b0b8a8`, line-height 1.85. Strong text in body gets `#eaede8` at weight 500."
- "Build a monospace badge: `rgba(138,173,110,0.08)` background, `#a4c48a` text, 100px radius, 6px 18px padding, IBM Plex Mono 11px uppercase, 0.08em letter-spacing, 1px solid `rgba(138,173,110,0.2)` border."
- "Create the site navigation: fixed top, 56px height, `rgba(8,10,8,0.85)` background with `backdrop-filter: blur(12px)`. Logo: Outfit 18px weight 800, 'Sync' in `#eaede8`, 'Engine' in `linear-gradient(135deg, #a4c48a, #a5b4fc)` via background-clip text. Links: Outfit 13px weight 500, `#8a9480`, hover `#eaede8`. Border-bottom: 1px solid `#242e22`."
- "Design a note/aside block: `#0f120f` background, 1px solid `#242e22` border, 16px radius, 24px padding. Header: Outfit 11px weight 700, uppercase, 0.08em tracking, `#5e6858`. Body: 15px Source Serif 4, `#b0b8a8`, line-height 1.85. Strong text: `#eaede8` weight 500."
- "Create a technical aside: `#0f120f` background, 1px solid `#242e22` border, `border-left: 3px solid var(--indigo)`, 16px radius, IBM Plex Mono 11px, `#8a9480`, line-height 1.8. Inline code: `rgba(129,140,248,0.08)` background, 3px radius."

### Iteration Guide
1. Always use the three-font pairing: Outfit (display/UI), Source Serif 4 (body/prose), IBM Plex Mono (labels/code)
2. Headings are heavy (800–900 Outfit), body is light (300 Source Serif 4), labels are medium (400–500 Mono) — never invert this weight relationship
3. Background gradient formula: always `linear-gradient(135deg, color1, color2)` — the 135deg angle is the system's signature diagonal
4. Card formula: `var(--card)` background + `1px solid var(--bd)` border + 16px radius — no shadow. Only hero CTAs and featured cards get shadows
5. Text colors descend: `--tx` → `--t2` → `--t3` → `--dim` — four tiers, never skip more than one level in a single component
6. Green-tint everything: backgrounds, borders, and text all carry a subtle green undertone — this is what makes the design feel organic rather than generic dark mode
7. Pill shapes (100px radius) for interactive elements at the hero level; 16px radius for containment; 12px for small interactive elements
8. Atmosphere orbs and grain provide environmental depth — content floats above this living substrate
9. Transitions use `cubic-bezier(0.16, 1, 0.3, 1)` (`var(--ease)`) for organic, slightly overshooting motion — not linear, not standard ease
