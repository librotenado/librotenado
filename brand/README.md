# librotenado – brand assets

**The mark:** a T-account, the oldest picture of bookkeeping. Two short strokes have been punched out of the
crossbar and stand beneath it – debit and credit, equal in length, so the account balances. Two corner marks, top
left and bottom right, frame it like an excerpt from a larger, open whole: one model, many formats. Small sizes
(16–48 px) use the punched T without the corners.

**What it is not:** an app icon. librotenado is a protocol; the mark stands free on its background. The square
icons exist only where platforms need one (touch icons, avatars).

Regenerate everything from code: `python3 brand/generate.py` (needs `rsvg-convert` from Homebrew `librsvg` and
`fontTools`). Edit `generate.py`, not the exported files.

## Colours

| Name | Hex | Use |
|---|---|---|
| Ink (navy) | `#14213D` | the T and text on light, dark backgrounds |
| Blue | `#1F5FAD` | the one accent on light: debit/credit strokes, corners, links |
| Paper | `#F4F6FA` | light backgrounds, the T and text on dark |
| Blue light | `#7FB2F0` | the accent on dark |
| Steel | `#4A5D70` | descriptor and secondary text on light |
| Mist | `#A9B6C8` | descriptor and secondary text on dark |
| Night | `#0E1729` | page background in dark mode – ink stays free for surfaces on it |

## Type

**Atkinson Hyperlegible Next** for the name, headings and text, **Atkinson Hyperlegible Mono** for the descriptor,
code and protocol notation (`chart_selected`, `librotenado/0.1`) – both designed for legibility, both under the SIL Open Font
License 1.1 (`fonts/`). Websites and documents bundle the fonts; nothing is loaded from other servers.

## Name and descriptor

- Always lowercase – **librotenado**, also at the start of a sentence and in titles.
- The descriptor is **open accounting protocol**. Its version is written plainly – **version 0.4** – and changes with
  every release. `librotenado/0.1` is not that version but the identifier of the harmonised chart of accounts (spec
  part 0004), which keeps its own version; it belongs in the specification and the documentation.
- The lockup with descriptor is the default; the name-only lockup is for tight spaces.

## Where to use what

| Use | File |
|---|---|
| Website `<head>` | `web/` – `favicon.ico`, `favicon.svg` (follows light/dark mode), `apple-touch-icon.png`, `icon-192.png`, `icon-512.png`, `site.webmanifest`, `social.png` |
| Mark, transparent | `svg/librotenado-mark-on-{light,dark}.svg`, `png/mark/` |
| Mark, tiny (≤ 48 px) | `svg/librotenado-mark-small-on-{light,dark}.svg` |
| Monochrome | `svg/librotenado-mark-{black,white}.svg` |
| Mark with name and descriptor | `svg/librotenado-lockup-on-{light,dark}.svg`, `png/lockup/` |
| Mark with name only | `svg/librotenado-lockup-name-on-{light,dark}.svg` |
| Websites with light and dark mode | `svg/librotenado-{mark,lockup,lockup-name}-adaptive.svg` – as `<img>` they follow the system setting; inlined, the page sets `--logo-ink`, `--logo-accent` and `--logo-sub` to follow its own switch |
| Name only | `svg/librotenado-wordmark-on-{light,dark}.svg`, `png/wordmark/` |
| Square icon (touch icons) | `svg/librotenado-icon{,-square,-small}.svg`, `png/icon*/` |
| GitHub organisation avatar | `png/social/github-avatar-500.png` |
| Social preview (`og:image`, GitHub) | `png/social/librotenado-social-1280x640.png` |

All text in the SVGs is converted to outlines; they need no font.

## Rights

librotenado's name and brand belong to falkOSS – Falcon Lake Open Source Software gUG (haftungsbeschränkt) i. G.,
the publisher of librotenado. They are **not** covered by the licences of the specification (CC BY 4.0) or the
library (MIT). Anyone may use the name and the mark to say that their software implements librotenado, as long as
it does not suggest endorsement by falkOSS; the mark may not be changed or used for other products. Conformance
rules for implementations will follow with the specification (OPEN).
