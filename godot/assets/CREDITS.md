# Asset credits

Third-party assets bundled with CyberHeist. Each folder keeps the original
licence file next to the assets it covers.

## Fonts

| Font | Use | Licence | Source |
|---|---|---|---|
| JetBrains Mono 2.304 (Regular, Medium, Bold) | Body text, card rules text | SIL OFL 1.1 (`fonts/jetbrains-mono/OFL.txt`) | https://www.jetbrains.com/lp/mono/ |
| Share Tech Mono | Menu and label text | SIL OFL 1.1 (`fonts/share-tech-mono/OFL.txt`) | https://fonts.google.com/specimen/Share+Tech+Mono |
| VT323 | Headings, wordmark, large numbers | SIL OFL 1.1 (`fonts/vt323/OFL.txt`) | https://fonts.google.com/specimen/VT323 |

## Backgrounds

### Zone backgrounds (`backgrounds/`)

From the free Cyberpunk Visual Novel Background Pack by **DreamBreaker
Designs** (`backgrounds/LICENSE.txt`). Source:
https://dreambreaker-designs.itch.io/free-cyberpunk-visual-novel-background-pack

Free to use, including commercially, with no attribution required. The author
states the pack was generated with Midjourney AI. Modified: cropped to
1280x720 and colour graded per zone.

| File | Zone | Original |
|---|---|---|
| `zone_1_corporate.jpg` | Corporate Network | `office-6.png` |
| `zone_2_bank.jpg` | Bank | `facility-2.png` |
| `zone_3_vault.jpg` | Data Vault | `facility-4.png` |

## Icons

### Tabler Icons (`ui/icons/tabler/`)

MIT licence (`ui/icons/tabler/LICENSE.txt`). Source: https://tabler.io/icons

Outline icons, modified: the metadata comment was removed and `currentColor`
was replaced with white so Godot can tint them with `modulate`.

### game-icons.net (`ui/icons/game-icons/`)

CC BY 3.0 (`ui/icons/game-icons/license.txt`). Source: https://game-icons.net

Modified: the black background square was removed so the icons are white on
transparent. Credit by author:

- **Delapouite** (https://delapouite.com): bank, cctv-camera, cpu, cyber-eye,
  database, firewall, keyboard, key-card, laptop, lockpicks, mesh-network,
  padlock-open, robot-antennas, security-gate, server-rack, spy, trojan-horse,
  wireframe-globe
- **Lorc** (http://lorcblog.blogspot.com): circuitry, domino-mask, hazard-sign,
  padlock, processor, radar-sweep, robot-golem, sentry-gun, virus
- **DarkZaitzev**: hooded-figure
- **Skoll**: siren
- **Andy Meneely**: police-badge

## Audio (`audio/`)

No third-party audio. Every sound effect and the background music loop were
synthesised for CyberHeist by `tools/generate_audio.py` (Python and numpy, no
samples or recordings), so they are original to this project and carry no
third-party licence or attribution requirement. Run the script again to
rebuild them.
