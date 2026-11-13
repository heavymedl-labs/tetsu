# Brand

Tetsu's mark is a schedule: letters and the T are booked time slots on an open
grid, and one slot runs hot in forge orange. `tetsu` (鉄) means iron.

| File | Use |
| --- | --- |
| `tetsu-wordmark-light.svg`, `tetsu-wordmark-dark.svg` | README header, switched by color scheme |
| `tetsu-avatar.svg`, `tetsu-avatar.png` | Organization avatar, 500 by 500 |
| `tetsu-social-preview.svg`, `tetsu-social-preview.png` | Repository social preview, 1280 by 640 |

| Color | Hex | Use |
| --- | --- | --- |
| Graphite | `#3A4250` | Mark on light backgrounds, avatar background |
| Steel | `#D9DEE5` | Mark on dark backgrounds |
| Forge orange | `#E5622B`, `#F07A45` on dark | The one booked slot that runs hot |
| Paper | `#F4F2EE` | Slots on the avatar |
| Night | `#16181C` | Social preview background |

[scripts/generate_brand.py](../../scripts/generate_brand.py) generates the SVGs.
To refresh the PNGs, render the SVGs at their own size, for example with
headless Chrome:

```bash
chrome --headless --hide-scrollbars --window-size=1280,640 \
  --screenshot=tetsu-social-preview.png tetsu-social-preview.svg
```

GitHub takes the avatar and social preview as uploads, not from the
repository: organization settings for the avatar, and repository settings
under Social preview for the preview.
