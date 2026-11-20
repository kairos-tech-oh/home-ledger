# Themes

The look of the app is made of four parts, chosen in **Settings → Appearance** and
kept per device:

| Part | Choices | What it changes |
| --- | --- | --- |
| Style | Terminal, Finance, Trading | Type, spacing, shapes: how panels, rows, tabs and buttons are drawn |
| Light or dark | Light, Dark, Device | Which set of colours is worn. The sun or moon beside Sync switches it in one click |
| Depth | Flat, Soft, Deep | How far panels, menus and pop-ups stand off the page |
| Colours | The style's own, or a colour theme | Every colour |

A **colour theme** is only colours, a light set and a dark set. It is laid over whichever
style is chosen, so one theme works with every style. Themes are kept in the ledger, so a
theme made on one computer is offered on all of them. Which theme a computer wears is still
its own choice.

## Making a theme

In Settings → Appearance, choose **New theme** (or **Copy as new** while wearing a theme). A
new theme starts as a copy of the colours on screen, in both modes. The editor puts the theme
on as you change it, so the whole app behind the pop-up shows the result, and a sample inside
the pop-up shows each piece the theme colours. Switch between **Light** and **Dark** at the
top to edit each set. **↺** sets a token back to the style's own colour. **Cancel** puts the
colours you had back on.

## Sharing a theme

**Export** saves the theme you are wearing as a `.json` file in Downloads and copies the same
text to the clipboard. **Import…** takes a file, or the pasted text, and adds the theme to the
ledger as a new one.

A theme file looks like this:

```json
{
  "homeLedgerTheme": 1,
  "name": "Seaside",
  "light": {
    "ground": "#f4f7f9",
    "surface": "#ffffff",
    "accent": "#0077cc"
  },
  "dark": {
    "ground": "#0d1b24",
    "surface": "#13262f",
    "accent": "#4fb3ff"
  }
}
```

Every colour is hex: `#rgb`, `#rrggbb`, or `#rrggbbaa` when it should be see-through. A token
left out keeps the style's own colour, so a theme can be as small as one accent. Anything that
is not a hex colour is dropped when the theme is saved.

## Tokens

| Group | Token | Used for |
| --- | --- | --- |
| Surfaces | `ground` | The page behind everything |
| | `surface` | Panels, cards, tiles and widgets |
| | `raised` | Rows and fields on a panel |
| | `raisedStrong` | The same, hovered or chosen |
| | `menu` | Menus, drop-downs and pop-ups |
| | `hairline` | Borders and rules |
| Text | `text` | Names and figures |
| | `dim` | Secondary text, and the drop-down arrow |
| | `faint` | Notes and placeholders |
| Accent | `accent` | Links, focus, the chosen tab, filled buttons |
| | `accentStrong` | A filled button when hovered. Mixed from `accent` if left out |
| | `onAccent` | Text on a filled button |
| Meaning | `positive` | Money held, gains |
| | `negative` | Money owed, losses, errors |
| | `warn` | Warnings |
| Pills | `pillBlue` `pillViolet` `pillGreen` `pillAmber` `pillRed` | Category pills |
| Depth | `shadow` | The colour shadows are cast in (alpha is ignored) |
| | `highlight` | The light that catches a raised thing's top edge |

The tints (a chosen tab's wash, a warning's box, a hover) are mixed from these, so they follow
a theme without tokens of their own. Colour still means something here: green is money held,
red is money owed. A theme that swaps them makes the screens harder to read.

## For developers

- `ui/src/theme.svelte.ts`: the choice, the token list (`TOKENS`, which is also the file
  format, so do not rename a token), and how a theme is worn: as inline custom properties on
  `<html>`, which beat any style's own.
- `ui/src/app.css`: Terminal, in light and dark, and the fallback for every token.
- `ui/src/theme-finance.css` and `ui/src/theme-trading.css`: the other styles, each scoped to
  `data-theme`.
- `ui/src/theme-depth.css`: depth, as one layer loaded after the styles. It sets only shadows
  and the sheen, never a colour, border or size, so a style is free to draw things its own way
  and still has depth.
- The ledger keeps themes under `themes`, read through `ledger_domain::themes::clean_themes`,
  which never refuses. They are written with the `theme-set` and `theme-delete` ops, which
  `hl` can send like any other.

A new screen or panel gets every theme and every depth for free, as long as it draws with the
shared classes (`.panel`, `.card`, `.tile`, `.row`, `.pill`) and the tokens instead of fixed
colours.
