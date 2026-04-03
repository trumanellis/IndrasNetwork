# Design System: SyncEngine

## 1. Visual Theme & Atmosphere

SyncEngine's interface is built on a minimal terminal aesthetic — a void-black canvas (`#0a0a0a`) where every element earns its place and decoration without purpose is removed. The experience evokes a well-designed command-line tool: focused, efficient, and unobtrusive.

The dual-font personality defines the system's character. Cormorant Garamond — a refined serif with italic weight — renders headlines in gold (`#d4af37`), giving section headers and page titles a sense of quiet authority. JetBrains Mono handles everything else: body text, buttons, inputs, labels, and metadata. This serif/mono pairing creates the tension between elegance and precision that defines the interface.

Color is used with extreme restraint. A three-color semantic system — moss green for status and success, cyan for interactivity, gold for importance — sits against near-total darkness. Text floats in three carefully calibrated opacity levels of off-white (`#f5f5f5`). Borders are barely visible (`#1a1a1a`), separating content through suggestion rather than force. The only loud color in the system is danger red (`#ff3366`), reserved for destructive actions and error states.

**Key Characteristics:**
- Void-black (`#0a0a0a`) background with near-invisible border separation (`#1a1a1a`)
- Cormorant Garamond italic in gold for all headings — literary, not corporate
- JetBrains Mono for all body/UI text — the interface speaks in monospace
- Three semantic accent colors only: moss (status), cyan (interactive), gold (important)
- Three type sizes only: 14px, 16px, 24px — nothing more
- Strict 4pt spacing grid
- Transparent button backgrounds with colored borders — no filled buttons
- 150–200ms transitions only; no decorative animation
- `prefers-reduced-motion` respected globally

## 2. Color Palette & Roles

### Backgrounds
- **Void Black** (`#0a0a0a`): Primary background. The deepest surface in the system.
- **Void Lighter** (`#0a0e0f`): Elevated surfaces — cards, inputs, code blocks. A barely perceptible lift above void-black.
- **Void Border** (`#1a1a1a`): Subtle borders and dividers. Visible only on careful inspection.

### Semantic
- **Gold** (`#d4af37`): Important — headings, section labels, modal titles. The system's voice of authority.
- **Cyan** (`#00d4aa`): Interactive — links, focus rings, peer IDs, inline code. Signals "you can act here."
- **Moss** (`#5a7a5a`): Status (inactive) — primary button borders, offline indicators. Quiet presence.
- **Moss Glow** (`#7cb87c`): Status (active) — online indicators, success messages, loading spinners. Alive and well.
- **Danger** (`#ff3366`): Destructive — errors, delete actions, error indicators. The only loud color.

### Text
- **Text Primary** (`#f5f5f5`): Primary text, button labels, body content.
- **Text Secondary** (`rgba(245, 245, 245, 0.7)`): Secondary text — card content, descriptions, input labels.
- **Text Muted** (`rgba(245, 245, 245, 0.5)`): Placeholders, hints, disabled text, metadata.

### State Backgrounds (Translucent)
- **Moss Tint** (`rgba(124, 184, 124, 0.1)`): Success message background, primary button hover fill.
- **Danger Tint** (`rgba(255, 51, 102, 0.1)`): Error message background, destructive button hover fill.
- **Cyan Tint** (`rgba(0, 212, 170, 0.1)`): Navigation link hover fill.
- **Overlay** (`rgba(0, 0, 0, 0.4)`): Modal backdrop.

## 3. Typography Rules

### Font Families
- **Headlines**: `'Cormorant Garamond', Georgia, serif` — italic, gold, letter-spacing 0.05em for page titles
- **Body / UI**: `'JetBrains Mono', 'SF Mono', 'Consolas', monospace` — all non-headline text

### Type Scale (3 sizes only)

| Role | Token | Size | Font | Color | Usage |
|------|-------|------|------|-------|-------|
| Large | `--text-lg` | 1.5rem (24px) | Cormorant Garamond, italic | Gold (`#d4af37`) | Page titles, section headers |
| Base | `--text-base` | 1rem (16px) | JetBrains Mono | Text Primary | Body text, inputs, buttons |
| Small | `--text-sm` | 0.875rem (14px) | JetBrains Mono | Text Secondary/Muted | Labels, metadata, hints, status |

### Typography Patterns
- **Page Title**: Cormorant Garamond, 2rem, weight 400, gold, letter-spacing 0.05em
- **Section Header**: Cormorant Garamond, 1.5rem, weight 400, italic, gold
- **Subsection Title**: JetBrains Mono, 0.875rem, uppercase, letter-spacing 0.1em, text-secondary
- **Peer ID**: JetBrains Mono, 0.875rem, cyan, word-break: break-all
- **Inline Code**: JetBrains Mono, 0.875rem, cyan, padding 2px 6px, void-lighter background

### Principles
- **Serif for soul**: Cormorant Garamond gives headlines a literary quality. Always italic for section headers, roman for page titles.
- **Mono for function**: JetBrains Mono is the universal UI voice. Everything actionable or informational is monospace.
- **Three sizes, no exceptions**: If you need hierarchy beyond sm/base/lg, use color or weight — never add a fourth size.
- **Line height**: 1.6 globally. The generous leading gives monospace text room to breathe.

## 4. Component Stylings

### Buttons

All buttons share: transparent background, `1px solid` border, 4px radius, JetBrains Mono at base size, `12px 16px` padding. Disabled state: `opacity: 0.5`, `cursor: not-allowed`.

**Primary (Moss)**
- Border: `var(--moss)` (`#5a7a5a`)
- Text: `var(--text-primary)` (`#f5f5f5`)
- Hover: border shifts to `var(--moss-glow)` (`#7cb87c`), background `rgba(124, 184, 124, 0.1)`

**Secondary (Void)**
- Border: `var(--void-border)` (`#1a1a1a`)
- Text: `var(--text-secondary)`
- Hover: border shifts to `var(--text-muted)`, text shifts to primary

**Destructive (Danger)**
- Border: `rgba(255, 51, 102, 0.5)`
- Text: `var(--danger)` (`#ff3366`)
- Hover: border shifts to full danger, background `rgba(255, 51, 102, 0.1)`

### Inputs
- Background: transparent
- Border: `1px solid var(--void-border)`
- Radius: 4px
- Padding: `12px 16px`
- Font: JetBrains Mono, base size
- Placeholder: `var(--text-muted)`
- Focus: border shifts to cyan, `box-shadow: 0 0 0 1px var(--cyan)`
- Disabled: `opacity: 0.5`
- Label: JetBrains Mono, small size, text-secondary, `margin-bottom: 8px`

### Cards
- Background: `var(--void-lighter)` (`#0a0e0f`)
- Border: `1px solid var(--void-border)`
- Radius: 4px
- Padding: `16px`
- Title: Cormorant Garamond, 1.5rem, italic, gold
- Content: text-secondary
- Grid: `auto-fill, minmax(280px, 1fr)`, gap 16px

### Status Indicators
- Layout: flex row, centered, gap 8px
- Dot: 8px circle
- Label: JetBrains Mono, small size, text-secondary
- **Online**: dot `var(--moss-glow)` (`#7cb87c`)
- **Connecting**: dot `var(--gold)` (`#d4af37`)
- **Offline**: dot `var(--moss)` (`#5a7a5a`)
- **Error**: dot `var(--danger)` (`#ff3366`)

### Modals
- Overlay: `rgba(0, 0, 0, 0.4)` centered flex container
- Container: max-width 480px, width 90%
- Background: `var(--void-black)`
- Border: `1px solid var(--gold)` — the only gold-bordered element
- Radius: 4px
- Padding: 24px
- Title: Cormorant Garamond, 1.5rem, italic, gold
- Actions: flex row, gap 12px, right-aligned

### Messages
- **Error**: `rgba(255, 51, 102, 0.1)` background, `1px solid var(--danger)`, danger text
- **Success**: `rgba(124, 184, 124, 0.1)` background, `1px solid var(--moss-glow)`, moss-glow text
- Both: 4px radius, `12px 16px` padding, JetBrains Mono small size

### Empty States
- Centered text, `var(--void-lighter)` background, bordered
- Icon: 1.5rem, text-muted
- Message: base size, text-secondary
- Hint: small size, text-muted

### Code Blocks
- Background: `var(--void-lighter)`
- Border: `1px solid var(--void-border)`
- Radius: 4px
- Padding: 16px
- Font: JetBrains Mono, small size, text-secondary
- `overflow-x: auto`, `white-space: pre`

### Loading
- Centered column layout
- Spinner: 24px circle, `2px solid var(--void-border)`, top border `var(--moss-glow)`, `animation: spin 1s linear infinite`
- Text: JetBrains Mono, small size, text-secondary

## 5. Layout Principles

### Spacing System (4pt Grid)

| Token | Value | Usage |
|-------|-------|-------|
| `--space-1` | 4px | Tight gaps — icon + label |
| `--space-2` | 8px | Default gap between related elements |
| `--space-3` | 12px | Padding inside small components (buttons, inputs) |
| `--space-4` | 16px | Padding inside cards, section gaps |
| `--space-6` | 24px | Major section spacing |
| `--space-8` | 32px | Page-level margins |

### Grid & Container
- Page max-width: 1200px, centered, padding 32px
- Card grid: `grid-template-columns: repeat(auto-fill, minmax(280px, 1fr))`, gap 16px
- Color grid: `auto-fill, minmax(200px, 1fr)`, gap 16px
- State grid: `auto-fill, minmax(300px, 1fr)`, gap 16px
- Button row: flex-wrap, gap 12px
- Status row: flex-wrap, gap 24px

### Whitespace Philosophy
- **Space creates hierarchy**: Generous whitespace separates sections better than visual weight. Borders are barely visible — spacing does the heavy lifting.
- **One action per screen**: Clear primary action, minimal competing elements.
- **Sections breathe**: Each section gets 32px bottom margin and padding, separated by a 1px void-border divider.

### Border Radius
- Single value: **4px** for everything — buttons, inputs, cards, code blocks, messages, modals. No variation.

## 6. Depth & Elevation

| Level | Treatment | Use |
|-------|-----------|-----|
| Flat (Level 0) | No border | Page background, text blocks |
| Standard (Level 1) | `1px solid var(--void-border)` | Cards, inputs, code blocks, nav, messages |
| Focus (Level 2) | `1px solid var(--cyan)` + `box-shadow: 0 0 0 1px var(--cyan)` | Focused inputs |
| Emphasis (Level 3) | `1px solid var(--gold)` | Modals — the only gold-bordered surface |

**Depth Philosophy**: SyncEngine uses a completely flat elevation system. There are no box-shadows for depth (except the cyan focus ring, which is functional, not decorative). Hierarchy is established through background color shifts (void-black → void-lighter), border visibility, and spacing — never through shadow or blur. This is a terminal, not a material surface.

### Focus Indicators
- `outline: 2px solid var(--cyan)`, `outline-offset: 2px` on `:focus-visible`
- Consistent across all interactive elements

## 7. Interaction & Motion

### Transitions (2 speeds only)

| Token | Duration | Easing | Usage |
|-------|----------|--------|-------|
| `--transition-fast` | 150ms | ease | Micro-interactions — hover color, focus border |
| `--transition-normal` | 200ms | ease | State changes — expand, modal entrance |

### Allowed Animations

| Name | Duration | Timing | Use |
|------|----------|--------|-----|
| `fadeIn` | 200ms | ease | Content appearing |
| `spin` | 1s | linear infinite | Loading spinner |
| `modal-appear` | 200ms | ease | Modal entrance |

### Hover States
- **Primary button**: border moss → moss-glow, background gains 10% moss tint
- **Secondary button**: border void → text-muted, text secondary → primary
- **Destructive button**: border strengthens to full danger, background gains 10% danger tint
- **Navigation links**: text shifts to cyan, background gains 10% cyan tint

### Accessibility
- `prefers-reduced-motion: reduce` sets all animation/transition durations to 0.01ms
- All interactive elements receive `:focus-visible` outline (2px cyan, 2px offset)

## 8. Responsive Behavior

### Grid Strategy
- All grids use CSS Grid `auto-fill` with `minmax()` — columns collapse naturally without breakpoints
- Card grid: 280px minimum → 1 column on mobile, 2–3 on desktop
- Color grid: 200px minimum
- State grid: 300px minimum

### Flex Wrapping
- Button rows, navigation links, status indicators, and spacing demos use `flex-wrap: wrap`
- Elements flow to new lines naturally at container edges

### Container Behavior
- Page: max-width 1200px with 32px padding — scales down on narrow viewports
- Modals: max-width 480px, width 90% — responsive by default
- Inputs: max-width 400px, 100% width

### Touch Targets
- Buttons: 12px 16px padding ensures comfortable tap targets
- Navigation links: 8px 12px padding with 4px radius hit area
- Inputs: 12px 16px padding for comfortable text entry

### Collapsing Strategy
- No explicit breakpoints — responsive behavior emerges from auto-fill grids and flex-wrap
- Card grids: 3-column → 2-column → 1-column based on 280px minimum
- Navigation: horizontal flex-wrap, links stack when container narrows
- Modals: 90% width ensures they scale on all screen sizes

## 9. Agent Prompt Guide

### Quick Color Reference
| Name | Hex | Role |
|------|-----|------|
| Background | `#0a0a0a` | Page background |
| Surface | `#0a0e0f` | Cards, inputs, code blocks |
| Border | `#1a1a1a` | Subtle dividers |
| Gold | `#d4af37` | Headings, important labels |
| Cyan | `#00d4aa` | Links, focus, interactive |
| Moss | `#5a7a5a` | Inactive status, button borders |
| Moss Glow | `#7cb87c` | Active status, success |
| Danger | `#ff3366` | Errors, destructive actions |
| Text | `#f5f5f5` | Primary text |
| Text 70% | `rgba(245,245,245,0.7)` | Secondary text |
| Text 50% | `rgba(245,245,245,0.5)` | Muted text, placeholders |

### Quick Font Reference
| Role | Family | Fallbacks |
|------|--------|-----------|
| Headlines | Cormorant Garamond | Georgia, serif |
| Body / UI | JetBrains Mono | SF Mono, Consolas, monospace |

### Quick Spacing Reference
| Token | Px | Use |
|-------|----|-----|
| `--space-1` | 4 | Icon gaps |
| `--space-2` | 8 | Element gaps |
| `--space-3` | 12 | Component padding |
| `--space-4` | 16 | Card padding |
| `--space-6` | 24 | Section spacing |
| `--space-8` | 32 | Page margins |

### Example Component Prompts
- "Create a card on `#0a0a0a` background. Card uses `#0a0e0f` fill with `1px solid #1a1a1a` border, 4px radius, 16px padding. Title in Cormorant Garamond 1.5rem italic `#d4af37`. Body in JetBrains Mono 1rem `rgba(245,245,245,0.7)`."
- "Build a primary button: transparent background, `1px solid #5a7a5a` border, 4px radius, 12px 16px padding, JetBrains Mono 1rem `#f5f5f5` text. Hover: border `#7cb87c`, background `rgba(124,184,124,0.1)`."
- "Design a text input: transparent background, `1px solid #1a1a1a` border, 4px radius, 12px 16px padding, JetBrains Mono 1rem `#f5f5f5`. Placeholder `rgba(245,245,245,0.5)`. Focus: border `#00d4aa` with `box-shadow: 0 0 0 1px #00d4aa`."
- "Create a modal: `rgba(0,0,0,0.4)` overlay, centered `#0a0a0a` box, max-width 480px, `1px solid #d4af37` border, 4px radius, 24px padding. Title in Cormorant Garamond 1.5rem italic gold. Action buttons right-aligned with 12px gap."
- "Build a status row: horizontal flex with 24px gap. Each indicator: 8px circle dot + JetBrains Mono 0.875rem label. Online dot `#7cb87c`, connecting `#d4af37`, offline `#5a7a5a`, error `#ff3366`."

### Do's and Don'ts

**Do:**
- Use transparent button backgrounds with colored borders
- Let spacing and background shifts create hierarchy
- Keep to exactly 3 type sizes (14px, 16px, 24px)
- Use gold only for headings and modal borders
- Use cyan only for interactive elements and focus states
- Respect the 4pt spacing grid — all values are multiples of 4

**Don't:**
- Add box-shadows for depth — this is a flat, terminal-inspired system
- Use filled/solid-background buttons
- Introduce additional type sizes or font weights
- Use color for decoration — every color has a semantic role
- Add decorative animations or transitions beyond the two allowed speeds
- Use border-radius values other than 4px
