# Design change process

The **Lattice Simulator** is the source of truth for the full UX flow.
One HTML5 app, two devices (Desktop + Mobile) on one simulated cloud.

- Design canvas: https://claude.ai/artifact/XgswugNGzHbdYaWjpYDDhe
- Simulator: https://claude.ai/artifact/5rwS5TZeyK6fziD1YRM8ak

## Any UI/UX change

1. **Discuss it first.** Write the proposal in chat or an issue. Wait for agreement.
2. **Update the design canvas** artboards.
3. **Update the simulator** to match.
4. **Update `docs/`**: the affected spec, and `docs/design-system/` if tokens or components change.
5. **Done** only when both devices still run the whole flow:
   sign in → pair worker (code or QR) → sessions → tabs → splits → shell → ports → agents (TTY / Visual / zoom) → agents overview → approvals → files as tabs → extensions.

## Design invariants (do not break)

- Dark by default. One accent. Accent marks focus, cursor, and primary action only.
- State = shape + hue. Never hue alone.
- No shadows in-window. Elevation = one step lighter surface + `line/strong` border.
- Focused pane = 1px inset accent ring. Nothing else glows.
- Nothing animates inside a terminal.
- Every tap target on mobile is ≥ 44px.
- Text contrast ≥ 4.5:1 on its surface.
- Primary button: at most one per surface. Labels are verbs.
- Every action has a keyboard chord. Mouse is a fallback.
